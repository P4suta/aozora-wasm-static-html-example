import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const editions = {
  aibiki: "000005-bb70c5adacb1c844",
  kumonoIto: "000092-d44233ef953c10da",
  sanshoDayu: "000689-254b86dff8d7f6df",
} as const;

test("serves the deterministic real-work lab without client scripts", async ({ page }) => {
  await page.goto("/index.html");
  await expect(page).toHaveTitle("aozora distribution verification lab");
  await expect(page.locator(".work-card")).toHaveCount(10);
  await expect(page.locator("script")).toHaveCount(0);
  await expect(page.locator('a[href="./build-report.json"]')).toBeVisible();

  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);
});

test("keeps resolved gaiji typographically neutral and unresolved gaiji identifiable", async ({
  page,
}) => {
  await page.goto(`/works/${editions.aibiki}.html`);
  const resolved = page.locator(".reader .aozora-gaiji[data-codepoint]").first();
  await expect(resolved).toBeVisible();
  const resolvedStyle = await resolved.evaluate((element) => {
    const style = getComputedStyle(element);
    const parent = element.parentElement;
    if (parent === null) throw new Error("resolved gaiji has no parent");
    return {
      color: style.color,
      parentColor: getComputedStyle(parent).color,
      backgroundColor: style.backgroundColor,
      borderBottomStyle: style.borderBottomStyle,
      borderBottomWidth: style.borderBottomWidth,
    };
  });
  expect(resolvedStyle.color).toBe(resolvedStyle.parentColor);
  expect(resolvedStyle.backgroundColor).toBe("rgba(0, 0, 0, 0)");
  expect(resolvedStyle.borderBottomStyle).toBe("none");
  expect(resolvedStyle.borderBottomWidth).toBe("0px");

  const unresolved = page.locator(".reader .aozora-gaiji[data-description]").first();
  await page.locator(".reader").evaluate((reader) => {
    const span = document.createElement("span");
    span.className = "aozora-gaiji";
    span.dataset["description"] = "unresolved fixture";
    span.textContent = "〓";
    reader.prepend(span);
  });
  const unresolvedStyle = await unresolved.evaluate((element) => {
    const style = getComputedStyle(element);
    const parent = element.parentElement;
    if (parent === null) throw new Error("unresolved gaiji has no parent");
    return {
      differsFromParent: style.color !== getComputedStyle(parent).color,
      hasBackground: style.backgroundColor !== "rgba(0, 0, 0, 0)",
      hasBorder: style.borderBottomStyle !== "none" && style.borderBottomWidth !== "0px",
    };
  });
  expect(
    unresolvedStyle.differsFromParent || unresolvedStyle.hasBackground || unresolvedStyle.hasBorder,
  ).toBe(true);
});

test("records the mixed-gaiji ruby compatibility boundary by WASM version", async ({ page }) => {
  await page.goto(`/works/${editions.kumonoIto}.html`);
  const reading = page.locator("rt", { hasText: /^かんだた$/ }).first();
  await expect(reading).toBeVisible();
  const base = await reading.evaluate((element) => {
    const ruby = element.parentElement;
    if (ruby === null || ruby.tagName !== "RUBY") throw new Error("ruby parent is missing");
    return Array.from(ruby.childNodes)
      .filter(
        (child) =>
          !(child instanceof HTMLElement) || (child.tagName !== "RT" && child.tagName !== "RP"),
      )
      .map((child) => child.textContent ?? "")
      .join("");
  });
  const footer = await page.locator(".site-footer").textContent();
  if (footer?.includes("aozora-wasm@0.5.0") || footer?.includes("aozora 0.5.0")) {
    expect(base).toBe("陀多");
    await expect(page.locator('[data-codepoint="U+728D"]').first()).toHaveText("犍");
  } else {
    expect(base).toBe("犍陀多");
  }
});

for (const viewport of [
  { width: 320, height: 720 },
  { width: 1440, height: 900 },
]) {
  test(`has no horizontal page overflow at ${viewport.width}px`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.goto(`/works/${editions.sanshoDayu}.html`);
    const dimensions = await page.evaluate(() => ({
      clientWidth: document.documentElement.clientWidth,
      scrollWidth: document.documentElement.scrollWidth,
    }));
    expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth + 1);
  });
}

test("the reading page passes automated accessibility checks", async ({ page }) => {
  await page.goto(`/works/${editions.aibiki}.html`);
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);
});
