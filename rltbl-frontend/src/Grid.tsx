import React from "react";
import {
  CellArray,
  CellClickedEventArgs,
  CompactSelection, DataEditor, DataEditorProps, DataEditorRef, DrawCellCallback, EditableGridCell, GridCell, GridCellKind,
  GridColumn,
  Rectangle,
  type Item
} from "@glideapps/glide-data-grid";

import range from "lodash/range.js";
import chunk from "lodash/chunk.js";
import "@glideapps/glide-data-grid/dist/index.css";
import { useLayer } from "react-laag";

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


export default function Grid(grid_args: { table: string, columns: any, rows: number }) {
  const table = grid_args.table;
  const columns = grid_args.columns;
  const rows = grid_args.rows;

  // console.log("TABLE", table);
  // console.log("COLUMNS", columns);

  const gridRef = React.useRef<DataEditorRef | null>(null);
  const dataRef = React.useRef<Row[]>([]);

  const getRowData = React.useCallback(async (r: Item) => {
    // await new Promise(res => setTimeout(res, 300));
    // return range(r[0], r[1]).map(rowIndex => [`1, ${rowIndex}`, `2, ${rowIndex}`]);
    // var data = [{"table":"table","path":"src/schema/table.tsv","type":"table","description":"the tables in this project"},{"table":"column","path":"src/schema/column.tsv","type":"column","description":"the columns for all of the tables"},{"table":"datatype","path":"src/schema/datatype.tsv","type":"datatype","description":"the datatypes for all of the columns"},{"table":"penguin","path":"src/data/penguin.tsv","type":"","description":"penguin measurement data"},{"table":"species","path":"src/schema/species.tsv","type":"","description":"the species in the study"},{"table":"region","path":"src/schema/region.tsv","type":"","description":"the geographical regions in the study"},{"table":"island","path":"src/schema/island.tsv","type":"","description":"the islands in the study"},{"table":"stage","path":"src/schema/stage.tsv","type":"","description":"the developmental stages in the study"},{"table":"sex","path":"src/schema/sex.tsv","type":"","description":"the biological sexes in the study"}];
    // const url = "https://example.org/products.json";
    // console.log("range", r, r[0], r[1]);
    const first = r[0];
    const last = r[1];
    const limit = last - first;
    const url = `/table/${table}.json?limit=${limit}&offset=${first}`;
    console.log("Fetch: " + url);
    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Response status: ${response.status}`);
      }
      const data = await response.json();
      return data["rows"];
    } catch (error) {
      console.error(error.message);
    }
  }, [table]);

  const cols = React.useMemo<readonly GridColumn[]>(() => {
    return columns;
  }, [columns])

  const async_args = useAsyncData<Row>(
    dataRef,
    50,
    5,
    getRowData,
    // toCell
    React.useCallback(
      (rowData, col) => ({
        kind: GridCellKind.Text,
        data: String(rowData.cells[columns[col].id].value),
        allowOverlay: true,
        displayData: String(rowData.cells[columns[col].id].text),
      }),
      [columns]
    ),
    // onCellEdited
    React.useCallback((cell, newVal, rowData) => {
      console.log("EDITED CELL", cell, newVal, rowData);
      const [col] = cell;
      if (newVal.kind !== GridCellKind.Text) return undefined;
      rowData.cells[columns[col].id].value = newVal.data;
      rowData.cells[columns[col].id].text = newVal.data;
      return rowData;
    }, [columns]),
    gridRef
  );

  const onCellsEdited = React.useCallback((newValues: readonly { location: Item; value: EditableGridCell }[]) => {
    console.log("EDITED CELLS", newValues);
  }, []);

  const onRowMoved = React.useCallback((from: number, to: number) => {
    console.log("ROW MOVED", from, to);
    // From https://github.com/glideapps/glide-data-grid/blob/main/packages/core/src/docs/examples/reorder-rows.stories.tsx
    // WARN: Might not be a good idea for large tables.
    const d = [...dataRef.current];
    const removed = d.splice(from, 1);
    d.splice(to, 0, ...removed);
    dataRef.current = d;
  }, [dataRef]);

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

  const [showMenu, setShowMenu] = React.useState<{ bounds: Rectangle; cell: Item, content: String }>();

  const onCellContextMenu = React.useCallback((cell: Item, event: CellClickedEventArgs) => {
    if (!dataRef.current) { return; }

    setShowMenu({ bounds: event.bounds, cell: cell, content: "<b>FOO</b>" });

    const [col, row] = cell;

    const rowData = dataRef.current[row];
    if (!rowData) { return; }
    const cellData = rowData.cells[columns[col].id];
    if (!cellData) { return; }
    console.log("onCellContextMenu", cellData, event);
    event.preventDefault();
  }, [columns, dataRef]);

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
      rowMarkers={"both"}
      onCellsEdited={onCellsEdited}
      onRowMoved={onRowMoved}
      //onCellClicked={onCellClicked}
      onCellContextMenu={onCellContextMenu}
      onPaste={true}
      fillHandle={true}
      // drawCell={drawCell}
      width="100%"
      height={window.innerHeight - 300}
      columns={cols}
      rows={rows}
    />
    {showMenu !== undefined &&
      renderLayer(
        <div
          {...layerProps}
          style={{
            ...layerProps.style,
            width: 300,
            padding: 4,
            borderRadius: 8,
            backgroundColor: "white",
            border: "1px solid black",
          }}>
          {showMenu.content}
        </div>
      )}
  </>;
}
