const separatorPattern = /^-{20,}$/;

export interface AozoraEnvelope {
  readonly preamble: string;
  readonly legend: string;
  readonly body: string;
  readonly bibliography: string;
}

function trimBlankEdges(lines: ReadonlyArray<string>): ReadonlyArray<string> {
  let start = 0;
  let end = lines.length;
  while (start < end && lines[start]?.trim() === "") start += 1;
  while (end > start && lines[end - 1]?.trim() === "") end -= 1;
  return lines.slice(start, end);
}

export function splitAozoraFile(source: string): AozoraEnvelope {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const separators = lines.flatMap((line, index) => (separatorPattern.test(line) ? [index] : []));
  if (separators.length !== 2) {
    throw new Error(`expected exactly two legend separators, found ${separators.length}`);
  }

  const legendStart = separators[0];
  const legendEnd = separators[1];
  if (legendStart === undefined || legendEnd === undefined) {
    throw new Error("legend separator indexes are unavailable");
  }

  const bibliographyStart = lines.findLastIndex(
    (line, index) => index > legendEnd && line.startsWith("底本："),
  );
  if (bibliographyStart === -1) {
    throw new Error("missing terminal bibliography beginning with 底本：");
  }

  const sections = {
    preamble: trimBlankEdges(lines.slice(0, legendStart)),
    legend: trimBlankEdges(lines.slice(legendStart + 1, legendEnd)),
    body: trimBlankEdges(lines.slice(legendEnd + 1, bibliographyStart)),
    bibliography: trimBlankEdges(lines.slice(bibliographyStart)),
  };

  for (const [name, section] of Object.entries(sections)) {
    if (section.length === 0) throw new Error(`empty source ${name}`);
  }

  return {
    preamble: sections.preamble.join("\n"),
    legend: sections.legend.join("\n"),
    body: sections.body.join("\n"),
    bibliography: sections.bibliography.join("\n"),
  };
}
