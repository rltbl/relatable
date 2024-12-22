import React from 'react';
import ReactDOM from 'react-dom/client';
import Grid from './Grid.tsx';

const table_name = window.rltbl.table.name;
const columns = Object.values(window.rltbl.columns).map(x => {
  return {title: x.label||x.name, id: x.name, grow: 1};
});
const rows = window.rltbl.range.total;

var portal = document.getElementById('portal');
const root = ReactDOM.createRoot(portal);
root.render(
  <React.StrictMode>
    <Grid table={table_name} columns={columns} rows={rows} />
  </React.StrictMode>
);

document.querySelectorAll(".table").forEach(el => el.remove());
document.querySelectorAll(".range").forEach(el => el.remove());
