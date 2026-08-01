const SEPARATOR = /^-{20,}$/;

function trimBlankEdges(lines) {
  let start = 0;
  let end = lines.length;
  while (start < end && lines[start].trim() === "") start += 1;
  while (end > start && lines[end - 1].trim() === "") end -= 1;
  return lines.slice(start, end);
}

export function splitAozoraFile(source) {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const separators = [];
  for (const [index, line] of lines.entries()) {
    if (SEPARATOR.test(line)) separators.push(index);
  }

  if (separators.length !== 2) {
    throw new Error(`expected exactly two legend separators, found ${separators.length}`);
  }

  const [legendStart, legendEnd] = separators;
  const bibliographyStart = lines.findLastIndex(
    (line, index) => index > legendEnd && line.startsWith("底本："),
  );
  if (bibliographyStart === -1) {
    throw new Error("missing terminal bibliography beginning with 底本：");
  }

  const preamble = trimBlankEdges(lines.slice(0, legendStart));
  const legend = trimBlankEdges(lines.slice(legendStart + 1, legendEnd));
  const body = trimBlankEdges(lines.slice(legendEnd + 1, bibliographyStart));
  const bibliography = trimBlankEdges(lines.slice(bibliographyStart));

  if (preamble.length === 0) throw new Error("empty source preamble");
  if (legend.length === 0) throw new Error("empty notation legend");
  if (body.length === 0) throw new Error("empty work body");
  if (bibliography.length === 0) throw new Error("empty bibliography");

  return {
    preamble: preamble.join("\n"),
    legend: legend.join("\n"),
    body: body.join("\n"),
    bibliography: bibliography.join("\n"),
  };
}
