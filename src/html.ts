import type { Work } from "./model.ts";

declare const safeHtmlBrand: unique symbol;
export type SafeHtml = string & { readonly [safeHtmlBrand]: true };

function trustedHtml(value: string): SafeHtml {
  return value as SafeHtml;
}

export function parserGeneratedHtml(value: string): SafeHtml {
  return trustedHtml(value);
}

export function escapeHtml(value: string): SafeHtml {
  return trustedHtml(
    value
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;"),
  );
}

function html(parts: TemplateStringsArray, ...values: ReadonlyArray<SafeHtml>): SafeHtml {
  return trustedHtml(
    parts.reduce((result, part, index) => result + part + (values[index] ?? ""), ""),
  );
}

function joinHtml(values: ReadonlyArray<SafeHtml>, separator = ""): SafeHtml {
  return trustedHtml(values.join(separator));
}

function contributorsHtml(contributors: Work["contributors"]): SafeHtml {
  return joinHtml(
    contributors.map(
      ({ role, name }) =>
        html`<p class="contributor"><span>${escapeHtml(role)}</span>${escapeHtml(name)}</p>`,
    ),
    "\n",
  );
}

function documentShell(input: {
  readonly title: string;
  readonly description: string;
  readonly body: SafeHtml;
}): string {
  return `<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="${escapeHtml(input.description)}">
  <title>${escapeHtml(input.title)}</title>
  <link rel="stylesheet" href="./styles/aozora-notation.css">
  <link rel="stylesheet" href="./styles/site.css">
</head>
<body>
${input.body}
</body>
</html>
`;
}

export function workPage(input: {
  readonly work: Work;
  readonly semanticHtml: SafeHtml;
  readonly bibliography: string;
  readonly version: string;
  readonly previous?: Work;
  readonly next?: Work;
}): string {
  const { work, semanticHtml, bibliography, version, previous, next } = input;
  const bibliographyHtml = joinHtml(
    bibliography.split("\n").map((line) => html`<p>${escapeHtml(line)}</p>`),
    "\n",
  );
  const previousLink = previous
    ? html`<a rel="prev" href="./${escapeHtml(previous.id)}.utf8.html">← ${escapeHtml(previous.title)}</a>`
    : trustedHtml('<span aria-hidden="true"></span>');
  const nextLink = next
    ? html`<a rel="next" href="./${escapeHtml(next.id)}.utf8.html">${escapeHtml(next.title)} →</a>`
    : trustedHtml('<span aria-hidden="true"></span>');

  const body = html`  <header class="site-header">
    <a href="./index.html">aozora-wasm static HTML example</a>
    <span>非公式</span>
  </header>
  <main>
    <article class="book">
      <header class="book-header">
        <p class="eyebrow">青空文庫記法から生成した静的HTML</p>
        <h1>${escapeHtml(work.title)}</h1>
        <p class="reading">${escapeHtml(work.reading)}</p>
        <div class="contributors">${contributorsHtml(work.contributors)}</div>
      </header>
      <div class="ornament" aria-hidden="true">＊　＊　＊</div>
      <section class="reader aozora-notation" aria-label="本文">
${semanticHtml}
      </section>
      <footer class="source-information">
        <h2>書誌と出典</h2>
        <div class="bibliography">${bibliographyHtml}</div>
        <ul class="source-links">
          <li><a href="${escapeHtml(work.cardUrl)}">青空文庫の作品カード</a></li>
          <li><a href="${escapeHtml(work.archiveUrl)}">公式のShift_JIS ZIP</a></li>
          <li><a href="./sources/${escapeHtml(work.id)}.txt" download>変換に使用したUTF-8原文</a></li>
        </ul>
      </footer>
    </article>
    <nav class="work-navigation" aria-label="作品間の移動">
      ${previousLink}
      <a href="./index.html">作品一覧</a>
      ${nextLink}
    </nav>
  </main>
  <footer class="site-footer">
    <p>Generated with <a href="https://www.npmjs.com/package/aozora-wasm">aozora-wasm@${escapeHtml(version)}</a></p>
    <p>青空文庫および各関係者による公式サービスではありません。</p>
  </footer>`;

  return documentShell({
    title: `${work.title} — aozora-wasm static HTML example`,
    description: `${work.title}をaozora-wasmで静的HTMLへ変換した非公式の参照例です。`,
    body,
  });
}

export function indexPage(input: {
  readonly works: ReadonlyArray<Work>;
  readonly version: string;
}): string {
  const cards = joinHtml(
    input.works.map(
      (
        work,
        index,
      ) => html`<li class="work-card${index === 0 ? trustedHtml(" featured") : trustedHtml("")}">
          <a href="./${escapeHtml(work.id)}.utf8.html">
            <span class="work-number">${escapeHtml(index === 0 ? "記事に登場した一篇" : work.id)}</span>
            <strong>${escapeHtml(work.title)}</strong>
            <span>${escapeHtml(work.contributors.map(({ name }) => name).join("／"))}</span>
          </a>
        </li>`,
    ),
    "\n",
  );

  const body = html`  <header class="site-header">
    <a href="./index.html">aozora-wasm static HTML example</a>
    <span>非公式</span>
  </header>
  <main class="index-main">
    <section class="hero">
      <p class="eyebrow">UTF-8青空文庫記法 → semantic HTML</p>
      <h1>パーサが担当する境界を、<br>十篇の小書架に。</h1>
      <p>公開npmパッケージ <code>aozora-wasm@${escapeHtml(input.version)}</code> をビルド時に利用し、原文位置や記法の意味を保ったHTMLを生成する最小のconsumer例です。</p>
      <p>検索、配信基盤、日次更新は扱いません。作品ファイルの外枠を分け、本文をパーサへ渡し、静的な読書ページへ組み立てるところだけを実装しています。</p>
    </section>
    <section aria-labelledby="works-heading">
      <div class="section-heading">
        <h2 id="works-heading">収録作品</h2>
        <span>10 works · 0 diagnostics</span>
      </div>
      <ol class="work-grid">${cards}</ol>
    </section>
    <section class="about" aria-labelledby="about-heading">
      <h2 id="about-heading">この例について</h2>
      <p><a href="https://zenn.dev/ksato9700/articles/550dd65bd2e679">青空文庫のUTF-8化についての記事</a>に登場した「あいびき」を起点に、テキスト置換ではなく青空文庫記法パーサでHTMLを生成しています。</p>
      <p>入力は青空文庫公式ZIPを文字コード変換しただけのUTF-8スナップショットです。外字やルビを事前置換せず、本文の解釈をaozora-wasmへ委ねています。</p>
      <p><a href="./build-report.json">再現性レポート</a> · <a href="https://github.com/P4suta/aozora-wasm-static-html-example">ソースコード</a></p>
    </section>
  </main>
  <footer class="site-footer">
    <p>青空文庫および各関係者による公式サービスではありません。</p>
  </footer>`;

  return documentShell({
    title: "aozora-wasm static HTML example",
    description:
      "公開npmパッケージaozora-wasmを使い、青空文庫の10作品を静的HTMLへ生成した非公式の参照実装です。",
    body,
  });
}
