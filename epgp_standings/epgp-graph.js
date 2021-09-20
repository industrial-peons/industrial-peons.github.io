// @ts-check

// TODO(enfluensa): tighten up this typing.
var Plotly;

/** @typedef {{ep: number, gp: number, timestamp: number}} Standing */
/** @typedef {{ep: number, gp: number, log: Standing[]}} PlayerInfo */
/** @type {(player: string) => Promise<PlayerInfo>} */
const standings = (() => {
  /** @type {Record<string, Promise<PlayerInfo>>} */
  let standings = {};

  return (/** @type {string} */ player) => {
    if (standings[player] == undefined) {
      standings[player] = fetch(`./players/${player}.json`).then((response) =>
        response.json()
      );
    }

    return standings[player];
  };
})();

const graph = document.getElementById("graph");

function sortTable(
  /** @type {HTMLTableElement} */ table,
  /** @type {number} */ idx,
  /** @type {boolean} */ ascending
) {
  const multiplier = ascending ? 1 : -1;
  const tbody = table.getElementsByTagName("tbody")[0];
  const rows = Array.from(tbody.getElementsByTagName("tr"));
  rows.sort((rowA, rowB) => {
    const valueA = rowA.getElementsByTagName("td")[idx].innerText;
    const valueB = rowB.getElementsByTagName("td")[idx].innerText;
    const numA = Number(valueA);
    const numB = Number(valueB);
    if (!isNaN(numA) && !isNaN(numB)) {
      return multiplier * (numA - numB);
    } else {
      return multiplier * valueA.localeCompare(valueB);
    }
  });

  tbody.innerHTML = "";
  for (const row of rows) {
    tbody.appendChild(row);
  }
}

for (const table of document
  .getElementById("standings_table")
  .getElementsByTagName("table")) {
  table.classList.add("is-hoverable");

  for (const headerRow of table
    .getElementsByTagName("thead")[0]
    .getElementsByTagName("tr")) {
    let idx = 0;
    let sortInfo = { index: 0, ascending: true, resets: [] };
    for (const header of headerRow.getElementsByTagName("th")) {
      const savedIdx = idx;
      const savedText = header.innerText;
      sortInfo.resets.push(() => (header.innerText = savedText));
      header.onclick = () => {
        for (const resetFunc of sortInfo.resets) {
          resetFunc();
        }

        if (sortInfo.index == savedIdx) {
          sortInfo.ascending = !sortInfo.ascending;
        } else {
          sortInfo.index = savedIdx;
          sortInfo.ascending = true;
        }

        sortTable(table, savedIdx, sortInfo.ascending);
        header.innerText += sortInfo.ascending ? " 🠕" : " 🠗";
      };

      idx++;
    }
  }

  /** @type {string | null} */
  let currentGraph = null;
  for (const row of table.getElementsByTagName("tr")) {
    row.onclick = () => {
      const td = row.getElementsByTagName("td")[0];
      if (currentGraph == td.innerText) {
        currentGraph = null;
        graph.style.display = "none";
      } else {
        const name = td.innerText;
        currentGraph = name;
        standings(name).then((playerInfo) => {
          if (currentGraph == name) {
            const playerData = playerInfo.log;
            graph.innerText = "";
            const x = playerData.map((data) => new Date(data.timestamp * 1000));

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
              {
                title: name,
              }
            );
          }
        });
        graph.style.display = "block";
      }
    };
  }
}
