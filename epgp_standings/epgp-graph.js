// @ts-check

/** @type {import('../third-party/mithril')} */
var m;

/** @typedef {{ep: number, gp: number, timestamp: number}} Standing */
/** @typedef {{ep: number, gp: number, log: Standing[]}} PlayerInfo */

fetch("./standings.json")
  .then((response) => response.json())
  .then((/** @type {Record<string, PlayerInfo>} */ standings) => {
    m.render(
      document.getElementById("contents"),
      m(
        "table",
        m("tr", m("th", "Name"), m("th", "EP"), m("th", "GP")),
        ...(() =>
          Object.keys(standings).map((name) =>
            m(
              "tr",
              m("td", name),
              m("td", standings[name].ep),
              m("td", standings[name].gp)
            )
          ))()
      )
    );
  });
