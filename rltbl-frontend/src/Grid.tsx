import React from "react";

import {
  CellArray,
  CellClickedEventArgs,
  CompactSelection,
  DataEditor, DataEditorProps, DataEditorRef,
  DrawCellCallback,
  EditableGridCell, GridCell, GridCellKind,
  GridColumn,
  GridSelection,
  Rectangle,
  type Item
} from "@glideapps/glide-data-grid";
import "@glideapps/glide-data-grid/dist/index.css";

import parse from 'html-react-parser';
import { debounce, isObject } from "lodash";
import range from "lodash/range.js";
import chunk from "lodash/chunk.js";
import { useLayer } from "react-laag";

import DropdownCell from "./Dropdown.tsx";

// Reltable types.
type Cell = {
  value: any,
  text: string,
}
type Row = {
  id: number,
  order: number,
  cells: Cell[],
};
type Column = {
  title: string,
  id: string,
  grow: number,
  kind: string,
};
type Cursor = {
  table: string,
  row: number,
  column: string,
};
type UserCursor = {
  name: string,
  color: string,
  cursor: Cursor,
  datetime: string,
};

type RowCallback<T> = (range: Item) => Promise<readonly T[]>;
type RowToCell<T> = (row: T, col: number) => GridCell;
type RowEditedCallback<T> = (cell: Item, newVal: EditableGridCell, rowData: T) => T | undefined;
function useAsyncData<TRowType>(
  dataRef: React.MutableRefObject<TRowType[]>,
  pageSize: number,
  maxConcurrency: number,
  getRowData: RowCallback<TRowType>,
  toCell: RowToCell<TRowType>,
  onEdited: RowEditedCallback<TRowType>,
  gridRef: React.MutableRefObject<DataEditorRef | null>
): Pick<DataEditorProps, "getCellContent" | "onVisibleRegionChanged" | "onCellEdited" | "getCellsForSelection"> {
  pageSize = Math.max(pageSize, 1);
  const loadingRef = React.useRef(CompactSelection.empty());

  const [visiblePages, setVisiblePages] = React.useState<Rectangle>({ x: 0, y: 0, width: 0, height: 0 });
  const visiblePagesRef = React.useRef(visiblePages);
  visiblePagesRef.current = visiblePages;

  const onVisibleRegionChanged: NonNullable<DataEditorProps["onVisibleRegionChanged"]> = React.useCallback(r => {
    setVisiblePages(cv => {
      if (r.x === cv.x && r.y === cv.y && r.width === cv.width && r.height === cv.height) return cv;
      return r;
    });
  }, []);

  const getCellContent = React.useCallback<DataEditorProps["getCellContent"]>(
    cell => {
      const [col, row] = cell;
      const rowData: TRowType | undefined = dataRef.current[row];
      if (rowData !== undefined) {
        return toCell(rowData, col);
      }
      return {
        kind: GridCellKind.Loading,
        allowOverlay: false,
      };
    },
    [dataRef, toCell]
  );

  const loadPage = React.useCallback(
    async (page: number) => {
      loadingRef.current = loadingRef.current.add(page);
      const startIndex = page * pageSize;
      const d = await getRowData([startIndex, (page + 1) * pageSize]);

      const vr = visiblePagesRef.current;

      const damageList: { cell: [number, number] }[] = [];
      const data = dataRef.current;
      for (const [i, element] of d.entries()) {
        data[i + startIndex] = element;
        for (let col = vr.x; col <= vr.x + vr.width; col++) {
          damageList.push({
            cell: [col, i + startIndex],
          });
        }
      }
      gridRef.current?.updateCells(damageList);
    },
    [dataRef, getRowData, gridRef, pageSize]
  );

  const getCellsForSelection = React.useCallback(
    (r: Rectangle): (() => Promise<CellArray>) => {
      return async () => {
        const firstPage = Math.max(0, Math.floor(r.y / pageSize));
        const lastPage = Math.floor((r.y + r.height) / pageSize);

        for (const pageChunk of chunk(
          range(firstPage, lastPage + 1).filter(i => !loadingRef.current.hasIndex(i)),
          maxConcurrency
        )) {
          await Promise.allSettled(pageChunk.map(loadPage));
        }

        const result: GridCell[][] = [];

        for (let y = r.y; y < r.y + r.height; y++) {
          const row: GridCell[] = [];
          for (let x = r.x; x < r.x + r.width; x++) {
            row.push(getCellContent([x, y]));
          }
          result.push(row);
        }

        return result;
      };
    },
    [getCellContent, loadPage, maxConcurrency, pageSize]
  );

  React.useEffect(() => {
    const r = visiblePages;
    const firstPage = Math.max(0, Math.floor((r.y - pageSize / 2) / pageSize));
    const lastPage = Math.floor((r.y + r.height + pageSize / 2) / pageSize);
    for (const page of range(firstPage, lastPage + 1)) {
      if (loadingRef.current.hasIndex(page)) continue;
      void loadPage(page);
    }
  }, [loadPage, pageSize, visiblePages]);

  const onCellEdited = React.useCallback(
    (cell: Item, newVal: EditableGridCell) => {
      const [, row] = cell;
      const current = dataRef.current[row];
      if (current === undefined) return;

      const result = onEdited(cell, newVal, current);
      if (result !== undefined) {
        dataRef.current[row] = result;
      }
    },
    [dataRef, onEdited]
  );

  return {
    getCellContent,
    onVisibleRegionChanged,
    onCellEdited,
    getCellsForSelection,
  };
}


export default function Grid(grid_args: { rltbl: any, height: number }) {
  const rltbl = grid_args.rltbl;
  const site = rltbl.site;
  const result = rltbl.result;
  const user = site.user.name;
  const table = result.table.name;
  const row_count = result.range.total;
  const height = grid_args.height;

  const columns: Column[] = Object.values(grid_args.rltbl.result.columns).map((x: any) => {
    var grow = 1;
    return {
      title: x.label || x.name,
      id: x.name,
      grow: grow,
      kind: x.kind,
      hasMenu: true
    };
  });

  // console.log("TABLE", table);
  // console.log("COLUMNS", columns);
  // console.log("SITE", site);
  const [cursor, setCursor] = React.useState<Cursor>({ table: table, row: 0, column: columns[0].id });

  const gridRef = React.useRef<DataEditorRef | null>(null);
  const dataRef = React.useRef<Row[]>([]);
  const cursorsRef = React.useRef<Map<string, string>>(new Map());
  const change_id = React.useRef<number>(0);

  const getCursors = React.useCallback((users: Map<string, UserCursor>) => {
    var cursors = new Map<string, string>();
    for (const user of Object.values(users)) {
      const cursor: Cursor = user.cursor;
      if (cursor.table !== table) { continue; };
      const row = cursor.row - 1;
      var col = 0;
      for (const [i, column] of columns.entries()) {
        if (cursor.column === column.id) {
          col = i;
          break;
        }
      }
      cursors[col + "," + row] = user.color;
    }
    return cursors;
  }, [table, columns]);

  const getRowData = React.useCallback(async (r: Item) => {
    const first = r[0];
    const last = r[1];
    const limit = last - first;
    const url = `/table/${table}.json?limit=${limit}&offset=${first}`;
    // console.log("Fetch: " + url);
    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Response status: ${response.status}`);
      }
      const data = await response.json();
      change_id.current = data["result"]["table"]["change_id"];
      cursorsRef.current = getCursors(data["site"]["users"]);
      return data["result"]["rows"];
    } catch (error) {
      console.error(error.message);
    }
  }, [change_id, table, cursorsRef, getCursors]);

  // Fetch data updated since we started.
  const pollData = React.useCallback(async () => {
    if (!dataRef.current) { return; }
    const url = `/table/${table}.json?_change_id=gt.${change_id.current}`;
    // console.log("Fetch: " + url);
    var rows: Row[] = [];
    const oldCursors = cursorsRef.current;
    var newCursors: Map<string, string> = new Map();
    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Response status: ${response.status}`);
      }
      const data = await response.json();
      change_id.current = data["result"]["table"]["change_id"];
      newCursors = getCursors(data["site"]["users"]);
      rows = data["result"]["rows"] as Row[];
    } catch (error) {
      console.error(error.message);
    }
    cursorsRef.current = newCursors;

    // Match rows to the grid by _id and re-render them.
    // TODO: Why do I need to updateCells? It should be automatic.
    const damageList: { cell: [number, number] }[] = [];
    for (const row of rows) {
      var r = 0;
      for (r = 0; r < row_count; r++) {
        const data = dataRef.current[r];
        if (!data) { continue; }
        if (data.id === row.id) { break; }
      }
      dataRef.current[r] = row;
      for (var c = 0; c < columns.length; c++) {
        damageList.push({
          cell: [c, r],
        });
      }
    }
    for (const key of Object.keys(oldCursors)) {
      const [c, r] = key.split(",", 2);
      damageList.push({
        cell: [parseInt(c), parseInt(r)],
      });
    }
    for (const key of Object.keys(newCursors)) {
      const [c, r] = key.split(",", 2);
      damageList.push({
        cell: [parseInt(c), parseInt(r)],
      });
    }
    gridRef.current?.updateCells(damageList);
  }, [table, columns, row_count, cursorsRef, getCursors, dataRef, gridRef]);

  // Poll for new data.
  React.useEffect(() => {
    const interval = setInterval(pollData, 5000);
    return () => clearInterval(interval);
  }, [pollData, dataRef]);

  const cols = React.useMemo<readonly GridColumn[]>(() => {
    return columns;
  }, [columns])

  const toCell: RowToCell<Row> = React.useCallback((rowData, col) => {
    const column_name = columns[col].id;
    const kind = columns[col].kind;
    if (kind === "dropdown") {
      const val = rowData.cells[columns[col].id].value;
      return {
        kind: GridCellKind.Custom,
        allowOverlay: true,
        copyData: val,
        data: {
          kind: "dropdown-cell",
          value: val,
          row: rowData.id,
          column: column_name,
          entry: null,
        },
      };
    }
    return {
      kind: GridCellKind.Text,
      data: String(rowData.cells[columns[col].id].value),
      allowOverlay: true,
      displayData: String(rowData.cells[columns[col].id].text),
    };
  }, [columns]);

  const onCellEdited: RowEditedCallback<Row> = React.useCallback((cell, newVal, rowData) => {
    // console.log("EDITED CELL", cell, newVal, rowData);
    const [col] = cell;
    var value = "UNDEFINED";
    if (newVal.kind === GridCellKind.Text) {
      value = newVal.data;
    } else if (newVal.kind === GridCellKind.Custom && newVal.data["kind"] === "dropdown-cell") {
      value = newVal.data["value"];
    }
    if (value === "UNDEFINED") return;
    rowData.cells[columns[col].id].value = value;
    rowData.cells[columns[col].id].text = value;

    return rowData;
  }, [columns]);

  const async_args = useAsyncData<Row>(
    dataRef,
    100,
    5,
    getRowData,
    toCell,
    onCellEdited,
    gridRef
  );

  const [gridSelection, setGridSelection] = React.useState<GridSelection>({
    rows: CompactSelection.empty(),
    columns: CompactSelection.empty(),
  });

  // Debounce postCursor to run after 1 second.
  // From https://www.developerway.com/posts/debouncing-in-react
  const postCursor = React.useCallback(() => {
    // console.log("POST CURSOR", cursor);
    try {
      fetch('/cursor', {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(cursor)
      });
    } catch (error) {
      console.error(error.message);
    }
  }, [cursor]);

  const postCursorRef = React.useRef<any>();
  React.useEffect(() => {
    postCursorRef.current = postCursor;
  }, [postCursor, postCursorRef]);

  const debouncedPostCursor = React.useMemo(() => {
    const func = () => {
      postCursorRef.current?.();
    }
    return debounce(func, 1000);
  }, [postCursorRef]);

  const onGridSelectionChange = React.useCallback((newSelection: GridSelection) => {
    if (newSelection.current) {
      const [col, row] = newSelection.current.cell;
      const cursor: Cursor = {
        table: table,
        row: row + 1,
        column: columns[col].id
      };
      // console.log("NEW CURSOR", cursor);
      setCursor(cursor);
      debouncedPostCursor();
    }

    setGridSelection(newSelection);
  }, [table, columns, debouncedPostCursor]);

  const onCellsEdited = React.useCallback((newValues: readonly { location: Item; value: EditableGridCell }[]) => {
    // console.log("EDITED CELLS BEFORE", newValues);
    try {
      newValues = rltbl.onCellsEdited(newValues);
    } catch (e) { /* pass */ }
    // console.log("EDITED CELLS AFTER", newValues);

    var changes: any[] = [];
    for (const entry of newValues) {
      var value = entry.value.data;
      if (isObject(value)) {
        value = value["value"];
      }
      changes.push({
        "type": "Update",
        row: entry.location[1] + 1,
        column: columns[entry.location[0]].id,
        value: value
      })
      onCellEdited(entry.location, entry.value, dataRef.current[entry.location[1]]);
    }
    const body = {
      action: "Do",
      table: table,
      user: user,
      description: "Set one value",
      changes: changes
    };
    // console.log("onCellsEdited body", body);
    try {
      fetch(`/table/${table}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
      });
    } catch (error) {
      console.error(error.message);
    }

  }, [rltbl, user, table, columns, dataRef, onCellEdited]);

  // const onRowMoved = React.useCallback((from: number, to: number) => {
  //   console.log("ROW MOVED", from, to);
  //   // From https://github.com/glideapps/glide-data-grid/blob/main/packages/core/src/docs/examples/reorder-rows.stories.tsx
  //   // WARN: Might not be a good idea for large tables.
  //   const d = [...dataRef.current];
  //   const removed = d.splice(from, 1);
  //   d.splice(to, 0, ...removed);
  //   dataRef.current = d;
  // }, [dataRef]);

  // const onCellClicked = React.useCallback((cell: Item, event: CellClickedEventArgs) => {
  //     if (!dataRef.current) { return; }
  //     setShowMenu(undefined);

  //     const [col, row] = cell;
  //     const rowData = dataRef.current[row];
  //     if (!rowData) { return; }
  //     const cellData = rowData.cells[columns[col].id];
  //     if (!cellData) { return; }
  //     console.log("onCellClicked", cellData, event);

  //     if (cellData.messages) {
  //         setShowMenu({bounds:event.bounds, cell: cell, content: String(cellData.messages)});
  //     }
  // }, [dataRef]);

  const [showMenu, setShowMenu] = React.useState<{ bounds: Rectangle; content: React.JSX.Element }>();

  const onCellContextMenu = React.useCallback((cell: Item, event: CellClickedEventArgs) => {
    // console.log("onCellContextMenu", cell, event);
    if (!dataRef.current) { return; }

    const [col, row] = cell;
    if (col === -1) {
      fetch(`/row-menu/${table}/${row + 1}`)
        .then((response) => { return response.text() })
        .then(text => {
          let content: React.JSX.Element = parse(text) as React.JSX.Element;
          setShowMenu({ bounds: event.bounds, content: content });
        });
    } else {
      const column = columns[col].id;
      fetch(`/cell-menu/${table}/${row + 1}/${column}`)
        .then((response) => { return response.text() })
        .then(text => {
          let content: React.JSX.Element = parse(text) as React.JSX.Element;
          setShowMenu({ bounds: event.bounds, content: content });
        });
    }

    event.preventDefault();
    return false;
  }, [table, columns, dataRef]);

  const onHeaderMenuClick = React.useCallback((col: number, bounds: Rectangle) => {
    const column = columns[col].id;
    fetch(`/column-menu/${table}/${column}`)
      .then((response) => { return response.text() })
      .then(text => {
        let content: React.JSX.Element = parse(text) as React.JSX.Element;
        setShowMenu({ bounds: bounds, content: content });
      });
    return false;
  }, [table, columns]);

  const { renderLayer, layerProps } = useLayer({
    isOpen: showMenu !== undefined,
    triggerOffset: 4,
    // onOutsideClick: () => {},
    onOutsideClick: () => setShowMenu(undefined),
    trigger: {
      getBounds: () => ({
        bottom: (showMenu?.bounds.y ?? 0) + (showMenu?.bounds.height ?? 0),
        height: showMenu?.bounds.height ?? 0,
        left: showMenu?.bounds.x ?? 0,
        right: (showMenu?.bounds.x ?? 0) + (showMenu?.bounds.width ?? 0),
        top: showMenu?.bounds.y ?? 0,
        width: showMenu?.bounds.width ?? 0,
      }),
    },
    placement: "bottom-start",
    auto: true,
    possiblePlacements: ["bottom-start", "bottom-end"],
  });

  const drawCell: DrawCellCallback = React.useCallback((args, draw) => {
    draw(); // draw up front to draw over the cell
    // if (!dataRef.current) { return; }
    // const { ctx, rect, col, row } = args;
    // const color = cursorRef.current[col + "," + row];
    // if (!color) { return; };
    // ctx.beginPath();
    // ctx.rect(rect.x + 1, rect.y + 1, rect.width - 1, rect.height - 1);
    // ctx.save();
    // ctx.strokeStyle = color;
    // ctx.stroke();
    // ctx.restore();
  }, []);

  // Draw a red triangle in upper-right, like Excel.
  // const drawCell: DrawCellCallback = React.useCallback((args, draw) => {
  //   draw(); // draw up front to draw over the cell
  //   if (!dataRef.current) { return; }
  //   const { ctx, rect, col, row } = args;
  //   const rowData = dataRef.current[row];
  //   if (!rowData) { return; }
  //   const cellData = rowData.cells[columns[col].id];
  //   if (!cellData) { return; }
  //   // if (cellData.message_level !== "error") { return; }
  //   const size = 7;
  //   ctx.beginPath();
  //   ctx.moveTo(rect.x + rect.width - size, rect.y + 1);
  //   ctx.lineTo(rect.x + rect.width, rect.y + size + 1);
  //   ctx.lineTo(rect.x + rect.width, rect.y + 1);
  //   ctx.closePath();
  //   ctx.save();
  //   ctx.fillStyle = "#ff0000";
  //   ctx.fill();
  //   ctx.restore();
  // }, [columns, dataRef]);

  return <>
    <DataEditor
      ref={gridRef}
      {...async_args}
      customRenderers={[DropdownCell]}
      rowMarkers={"clickable-number"}
      gridSelection={gridSelection}
      onGridSelectionChange={onGridSelectionChange}
      onCellsEdited={onCellsEdited}
      // onRowMoved={onRowMoved}
      //onCellClicked={onCellClicked}
      onCellContextMenu={onCellContextMenu}
      onHeaderMenuClick={onHeaderMenuClick}
      // onPaste={true}
      // fillHandle={true}
      drawCell={drawCell}
      width="100%"
      height={height}
      columns={cols}
      rows={row_count}
    />
    {showMenu !== undefined &&
      renderLayer(
        <div
          {...layerProps}
          style={{
            ...layerProps.style,
          }}>
          {showMenu.content}
        </div>
      )}
  </>;
}
