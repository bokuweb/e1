# e1 UI Specification

> Companion to [`roadmap.md`](roadmap.md). The roadmap says *what* we build and when; this document says *what it looks like* and *which components render it*. Where this document is silent, Ginka's `docs/ui.md` applies: the window, the tokens and the header strips are the same by design (roadmap §4.3).
> Last updated: 2026-09-05

## 1. Design direction

The same three-column workstation as Ginka, on the same dark glass:

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ ●●●  ⬓        │ ⇄ bokuweb/ginka · Pull requests   [open][closed]  ⟳  ⬓          │
├───────────────┼───────────────────────────────────────┼─────────────────────────┤
│ e1   bokuweb  │ ⇄ Start a chat before it has a…  #12  │ #12 Start a chat before  │
│               │   bokuweb · 2h · 3 comments           │ ⇄ Open · bokuweb wants  │
│ ◫ Inbox    4  │ ⇄ Add a project, and start a…    #11  │   to merge feat → main  │
│ ⇄ My pulls    │   bokuweb · 1d                        │   +412 −38 · 9 files    │
│ ◎ Reviews     │ ⇄ Send a follow-up into the…     #10  │ ─────────────────────── │
│ ◌ Assigned    │   bokuweb · 2d                        │ (markdown body)         │
│               │                                       │                         │
│ Repositories  │                                       │ ─────────────────────── │
│ ▸ bokuweb/…   │                                       │ ◯ alice · 2h            │
│ ▸ bokuweb/…   │                                       │   Looks good, one nit…  │
│               │                                       │                         │
│ (B) bokuweb   │                                       │        [Open on GitHub] │
└───────────────┴───────────────────────────────────────┴─────────────────────────┘
```

The five properties of Ginka's §1 hold — glass, chromeless, density with air, ambient status, short motion — with one addition:

6. **State is a glyph, not a word.** Open, closed, merged and draft are the four marks GitHub users already read at a glance, drawn in the status colours and never as text alone. A row says its state in the first 16 px.

## 2. Design tokens

Identical to Ginka's `docs/ui.md` §2, from the same `assets/themes/*.json`. The mapping that matters here:

| State | Token | Glyph |
| --- | --- | --- |
| Open pull / open issue | `status.done` | pull-request arrow / circle-dot |
| Draft pull | `text.muted` | pull-request arrow, hollow |
| Merged | `accent` | merge mark |
| Closed pull (unmerged) | `status.error` | pull-request, struck |
| Closed issue | `accent` | circle with a check |
| Unread notification | `accent` | 6 px dot before the title |

Geometry as Ginka's: 44 px header strips, 4 px grid, row radius 9, sidebar 250 (200–400), right panel 420 (280–720).

## 3. Regions

### 3.1 Headers — there is no title bar

As Ginka §3.1: each column paints itself to the top and carries a 44 px strip; the leading strip leaves 78 px for the traffic lights; every strip drags the window and double-clicks to zoom. The centre strip says what the list is — repository and kind, or the section name — and carries the open/closed toggle, refresh, and the right panel toggle.

### 3.2 Sidebar — navigation

- **Header** — app name (bold), then the viewer's login muted, or *not signed in*.
- **Sections** — four fixed rows, each an icon and a label, with a right-aligned count when it is known (unread for the inbox): Inbox, My pulls, Reviews (review requested), Assigned. Selected = `row.active` fill.
- **Repositories** — a small muted label, then one row per repository the viewer can reach, most recently pushed first: `owner/name` truncated from the left, a lock glyph when private. Picking one lists its pulls; the kind toggle is in the centre strip.
- **Footer** — avatar initial and login.

### 3.3 Centre — the list

A `uniform_list` of two-line rows at 56 px:

1. State glyph, title (truncated), right-aligned `#number`.
2. Author, age, comment count, and up to three labels as small chips coloured from the label's own colour at 22 % over the glass.

An inbox row is the same shape with the reason (`review requested`, `mention`, `subscribed`) where the author would be, and the unread dot. Empty and error states are one muted line each; loading over a stale list keeps the list and dims nothing — the refresh glyph spins instead.

### 3.4 Right panel — the item

- **Header** — `#number` and the title at 15/500; under it the state glyph and word, the author, and for a pull `base ← head`, `+adds −dels`, `n files`.
- **Labels** — the chips from the row.
- **Body** — markdown at the transcript measure (`TextView::markdown`).
- **Comments** — a hairline, then each comment as avatar initial, login, age, and its markdown body.
- **Footer** — *Open on GitHub*, which opens the browser; every item is one click from the real thing.

Empty state: *Pick something to read* over the glass.

## 4. Component mapping

| Region | Component |
| --- | --- |
| Window shell | `gpui-component` `Root`, our header strips |
| Columns | `h_resizable` + `resizable_panel` |
| List | `gpui::uniform_list` with rows from `e1_ui::rows::ItemRow` |
| Markdown | `gpui-component` `TextView::markdown` |
| Tooltips, icons | `gpui-component` primitives; our SVGs in `assets/icons/` for what the toolkit lacks (pull request, merge, issue, comment, lock) |

## 5. Interaction rules

- `⌘B` sidebar, `⌘⌥B` right panel, `⌘R` refresh what is on screen. `⌘K` lands in M3.
- Picking a row opens it on the right and never navigates the centre away.
- Never block: every fetch shows the stale value until the fresh one lands.
- Truncate repository names and titles from the left only when the tail is the meaningful part (repository names); titles truncate from the right.
