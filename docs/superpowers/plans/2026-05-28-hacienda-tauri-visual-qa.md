# Hacienda Tauri Visual QA Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add repeatable visual and interaction checks for the Tauri workbench UI after the walking skeleton is functional.

**Architecture:** Use Playwright against the Vite dev server for deterministic UI states and use the Codex in-app Browser plugin for manual screenshots of the local app during development. Keep these checks focused on layout integrity, no text overlap, responsive behavior, and critical workflow actions.

**Tech Stack:** Playwright, Vite, React, Browser plugin screenshots, npm scripts.

---

## Scope

In scope:

- Desktop and narrow viewport screenshots.
- Dashboard, matter detail, document atelier, PII panel.
- Text overflow checks.
- Button state and editor save interaction.

Out of scope:

- Native Tauri window automation.
- Pixel-perfect snapshot gating for every component.
- End-to-end SharePoint live auth.

## Files

- Create `apps/hacienda-workbench/tests/ui/workbench.spec.ts`
- Create `apps/hacienda-workbench/playwright.config.ts`
- Modify `apps/hacienda-workbench/package.json`
- Add fixture JSON under `apps/hacienda-workbench/tests/fixtures/`
- Add UI test seam in `apps/hacienda-workbench/src/api.ts`

## Tasks

### Task 1: Playwright Setup

- [ ] Add dev dependency `@playwright/test`.
- [ ] Add scripts:

```json
{
  "test:ui": "playwright test",
  "test:ui:update": "playwright test --update-snapshots"
}
```

- [ ] Add config with Vite webServer on port 1420.
- [ ] Run `npx playwright install chromium`.

### Task 2: API Test Seam

- [ ] In `api.ts`, if `import.meta.env.VITE_MOCK_API === "1"`, return fixture-backed API functions.
- [ ] Fixtures include one matter, two working documents, and PII spans.
- [ ] Test seam must not be enabled in production build.

### Task 3: Layout Tests

- [ ] Add tests:

```text
dashboard renders without horizontal overflow at 1440x900
document atelier renders without overlap at 1280x820
narrow viewport keeps editor usable at 390x844
PII tokens remain visible and not clipped
```

- [ ] Run `npm run test:ui`.

### Task 4: Interaction Tests

- [ ] Test selecting a document updates editor text.
- [ ] Test save button calls mock save and increments revision label.
- [ ] Test empty state has no layout jump.

### Task 5: Manual Browser Plugin Pass

- [ ] Start app dev server.
- [ ] Use Browser plugin to open `http://localhost:1420`.
- [ ] Capture desktop and mobile screenshots.
- [ ] Verify no overlapping text, no blank panels, no oversized marketing layout.

## Verification

Run:

```powershell
Set-Location apps\hacienda-workbench
npm run build
npm run test:ui
Set-Location ..\..
```

Acceptance:

```text
UI test suite passes on desktop and mobile viewports.
Editor, PII panel, and document list are visible without overlap.
Manual Browser screenshot confirms the primary screen is the usable workbench.
```
