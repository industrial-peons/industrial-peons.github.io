// @ts-check

// TODO(enfluensa): tighten up this typing.
var Plotly;

/** @typedef {{ep: number, gp: number, timestamp: number}} Standing */
/** @typedef {{ep: number, gp: number, log: Standing[]}} PlayerInfo */
/** @type {() => Promise<Record<string, PlayerInfo>>} */
const standings = (() => {
  let standings = null;

  return () => {
    if (standings != null) {
      return standings;
    }
    standings = fetch("./standings.json").then((response) => response.json());
    return standings;
  };
})();

const graph = document.getElementById("graph");

for (const table of document
  .getElementById("standings_table")
  .getElementsByTagName("table")) {
  /** @type {string | null} */
  let currentGraph = null;
  for (const row of table.getElementsByTagName("tr")) {
    for (const td of row.getElementsByTagName("td")) {
      row.onclick = () => {
        if (currentGraph == td.innerText) {
          currentGraph = null;
          graph.style.display = "none";
        } else {
          const name = td.innerText;
          currentGraph = name;
          standings().then((standings) => {
            if (currentGraph == name) {
              const playerData = standings[name].log;
              graph.innerText = "";
              const x = playerData.map(
                (data) => new Date(data.timestamp * 1000)
              );

              function plotLine(
                /** @type {string} */ name,
                /** @type {number[]} */ y
              ) {
                return {
                  x,
                  y,
                  mode: "lines",
                  line: { shape: "hv" },
                  name,
                };
              }

              Plotly.newPlot(
                graph,
                [
                  plotLine(
                    "EP",
                    playerData.map((data) => data.ep)
                  ),
                  plotLine(
                    "GP",
                    playerData.map((data) => data.gp)
                  ),
                  plotLine(
                    "PR (x10)",
                    playerData.map((data) => (data.ep * 10) / data.gp)
                  ),
                ],
                { title: name }
              );
            }
          });
          graph.style.display = "block";
        }
      };
      break;
    }
  }
}
