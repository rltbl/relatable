import React from 'react';
import ReactDOM from 'react-dom/client';
import Grid from './Grid.tsx';

var portal = document.getElementById('portal');
const height = window.innerHeight - portal.getBoundingClientRect().top - 5;
const root = ReactDOM.createRoot(portal);
root.render(
  <React.StrictMode>
    <Grid rltbl={window.rltbl} height={height} />
  </React.StrictMode>
);

document.querySelectorAll(".table").forEach(el => el.remove());
document.querySelectorAll(".range").forEach(el => el.remove());
