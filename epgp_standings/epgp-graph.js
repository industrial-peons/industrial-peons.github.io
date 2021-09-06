// @ts-check

/** @type {import('../third-party/mithril')} */
var m;

/** @typedef {{ep: string, gp: string, timestamp: number}} Standing */

fetch("./standings.json")
  .then((response) => response.json())
  .then((/** @type {Record<string, Standing[]>} */ standings) => {
    const contentsDiv = document.getElementById("contents");
    m.render(contentsDiv, m("pre", JSON.stringify(standings, undefined, 2)));
  });
