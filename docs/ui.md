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

Geometry as Ginka's: 44 px header strips, 4 px grid, row radius 9, sidebar 250 (200–400), right panel 420 (280–720). The two sized columns are `flex_none`: only the centre grows into what the window has, which is what keeps one divider from moving the other.

The palette is Ginka's with the saturation eased a touch (backgrounds ×0.85, accent and status ×0.92, text ×0.9): the same hues, a little less of them, so a full day in the window does not tire.

### Type

The system UI font, at the reference's sizes: sidebar rows and list titles 14 px, metadata 12 px, the detail body and comments 15 px on a 1.6 line height, code and paths in the mono family at 12 px.

## 3. Regions

### 3.1 Headers — there is no title bar

As Ginka §3.1: each column paints itself to the top and carries a 44 px strip; the leading strip leaves 78 px for the traffic lights; every strip drags the window and double-clicks to zoom. The centre strip says what the list is — repository and kind, or the section name — and carries the open/closed toggle, refresh, and the right panel toggle.

### 3.2 Sidebar — navigation

- **Header** — the viewer's avatar at 20 px (their initial until it arrives) and their login in bold, or *not signed in* muted. No app name and no logo: a window's title is what it is showing, and the person whose inbox this is says more than the app's name would.
- **Sections** — four fixed rows, each an icon and a label, with a right-aligned count when it is known (unread for the inbox): Inbox, My pulls, Reviews (review requested), Assigned. Selected = `row.active` fill.
- **Repositories** — a small muted label, then the repositories grouped under their owner, the way the reference groups chats under a project: an owner heading (chevron, the owner's name muted, a count) that folds its group and remembers that it did, and under it one row per repository, indented, the name alone since the heading already says whose, with a lock glyph when private. Owners are ordered by their most recently pushed repository, and the repositories inside the same way. Picking one lists its pulls; the kind toggle is in the centre strip.
- **Footer** — the viewer's avatar at 24 px (their initial until it arrives) and login, then the appearance control (a moon, a sun, or half of each for *follow the system*; a click moves to the next), and the sign-out mark.

### 3.3 Centre — the list, the finder, or a search

The centre strip carries chips for a repository — *Pull requests*, *Issues*, *Files* — then *Open*/*Closed* for the two lists, then a search box (240 px) that runs GitHub's issue search on ⏎ and lists the answer with no sidebar row highlighted.

**The finder** (the *Files* chip) is an input over a virtualized list of every file path in the repository, matched fuzzily and case-insensitively as the reader types, the directory muted and the file name in `text.secondary`. Picking a path reads the file into the right panel. When GitHub cut the tree short the finder says so in `status.attention` under the input.

A `uniform_list` of two-line rows at 56 px:

1. State glyph, title (truncated), right-aligned `#number`.
2. Author, age, comment count, and up to three labels as small chips coloured from the label's own colour at 22 % over the glass.

An inbox row is the same shape with the reason (`review requested`, `mention`, `subscribed`) where the author would be, and the unread dot. Empty and error states are one muted line each; loading over a stale list keeps the list and dims nothing — the refresh glyph spins instead.

### 3.4 Right panel — the item

- **Header** — `#number` and the title at 15/500; under it the state glyph and word, the author with their avatar at 18 px, and for a pull `base ← head`, `+adds −dels`, `n files`. Then the actions: *Merge* (filled, only for an open, non-draft pull GitHub does not call unmergeable; the first press arms it as *Merge now?*), and *Close* or *Reopen*. While a write is in flight the row says *Working…*; a refusal is GitHub's own words in `status.error` beside the buttons.
- **Labels** — the chips from the row.
- **Body** — markdown at the transcript measure (`TextView::markdown`).
- **Comments** — a hairline, then each comment as avatar (20 px, or the initial in a tinted circle until it arrives), login, age, and its markdown body. Under the last one, the composer: a card holding a textarea that grows from two to eight rows, and a filled *Comment* button; ⌘⏎ also sends.
- **Tabs** — for a pull only: *Conversation* and *Files n*, as chips under the header. Files is one virtualized list: each file is a 22 px header row on `bg.raised` (fold chevron, a one-letter status mark in the status colours, the path in mono, `+n −m`) followed by its diff, line-numbered both sides in mono at 12 px, added and removed lines tinted by `status.done` and `status.error` at 12 %, hunk headers on `code.bg`. Everything starts unfolded; a header folds its file.
- **A file** — when the finder opened one: the path in mono, the repository, size and line count, then the lines in a virtualized list with numbers in the gutter. A binary or oversized file is one sentence and the *Open on GitHub* control.
- **Markdown** — links in the accent, table heads as a translucent dark band (`black` at 35 %) over the glass with `text.secondary`, rows separated by `border.subtle`, inline code on `bg.surface`.
- **Footer** — *Open on GitHub*, in the right strip, which opens the browser; every item is one click from the real thing.

Empty state: *Pick something to read* over the glass.

### 3.5 Sign-in screen

What the centre column is when there is no token: the logo at 56 px, *Sign in to GitHub*, one sentence on what the app reads, and one filled button. Pressing it swaps the button for the device code in mono at 24 px inside a card, the address to enter it at, *Open in browser* (which also copies the code) and *Copy code*, and a quiet *Waiting for GitHub…* line. A refusal or an expiry is one sentence in `status.error` with *Try again*. Signing out is the small mark beside the login in the sidebar footer.

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

## 8. The logo

`assets/icons/e1.svg`: an uppercase `E` and a `1` in strokes at 2.2 on the 24-grid, one colour. It is painted in `Tokens::logo()` — white on the dark theme, navy (`#1E1B4B`) on the light one — which is not a token because no other part of the window uses it and a theme file should not have to name the logo. It sits on the sign-in screen at 56 px, and nowhere else in the window.
