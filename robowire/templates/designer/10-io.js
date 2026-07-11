// designer/10-io.js — import/export dialog elements
  const dlg = document.getElementById("ioDialog");
  const nameEl = document.getElementById("nlname");
  nameEl.addEventListener("change", () => { nl.name = nameEl.value; refresh(); });

