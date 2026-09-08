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

Geometry as Ginka's: 44 px header strips, 4 px grid, row radius 9, controls (buttons, fields) at 6 — a step under a row, because a control sits inside a card and matching the card's corner reads as a card in a card — sidebar 250 (200–400), right panel 420 (280–720). The two sized columns have explicit widths and the centre takes the rest (never under 320 px); a divider is a 9 px grab area centred on the column's edge, with a hairline in the accent while the pointer is over it or holding it; while it is held the pointer is tracked at the window, so crossing a text field or a scrollbar does not drop the drag. The sidebar may be 200–480 px; the right panel is bounded only by the centre's floor. Opening or closing a column slides it over the standard 260 ms with an ease-out cubic.

The palette is Ginka's with the saturation eased twice over (backgrounds ×0.72, accent and status ×0.83, text ×0.81): the same hues, a good deal less of them, so a full day in the window does not tire. The sidebar is **frosted**: not a second coat of dark over the window (`bg.sidebar` was `#120D19` at 70 %) but a milky tint — white at 6 % on the dark theme — over a window that now lets the desktop through at 72 %, so the left column reads as etched glass beside the clearer centre.

**The light theme is not the dark one inverted.** Its ground is a cool near-white (`#F7F8FC`) at 95 %, not a violet one, and it is nearly opaque where the dark theme is glass at 72 %: a light window that lets a wallpaper through takes that wallpaper's cast over every surface, and what came through was a purple wash. Its sidebar tint is the mirror of the dark theme's — near-black at 4 % rather than white at 6 % — so the left column sits a shade *back* from the content, which is where a light-theme sidebar belongs. The status colours are GitHub's light set (`#1A7F37`, `#CF222E`, `#9A6700`, `#BF3989`), which are made to be read on white, and the accent deepens to `#6B3FD4` for the same reason.

Two things are derived per theme rather than written in the file, because one number cannot serve both grounds. **Row tints** (`row.hover`, `row.active`) are the accent at 8 %/14 % on the light theme against 14 %/22 % on the dark: a tint reads against a dark ground by adding light, which is gentle, and against a light one by adding colour, which at the same strength is a stain. A **table's head** is black at 5 % on the light theme against 35 % on the dark, since the text over it is black too.

**A label's colour is moved until it reads.** GitHub's label colours are chosen against GitHub's background, so taken as written half of them disappear on ours — `enhancement`'s pale cyan on a light ground, a deep blue on a dark one. The hue carries the meaning and is kept; the lightness is not. On the light theme the chip is the label's hue at 88 % lightness with its text at most 32 %; on the dark theme it is the hue at 22 % alpha with its text at least 66 %.

### Type

The system UI font, a step under the reference: sidebar rows, list titles and the toolkit's base size 13 px, metadata 11.5 px, the detail body and comments 14 px on a 1.6 line height, code and paths in the mono family at 12 px.

## 3. Regions

### 3.1 Headers — there is no title bar

As Ginka §3.1: each column paints itself to the top and carries a 44 px strip; the leading strip leaves 78 px for the traffic lights; every strip drags the window and double-clicks to zoom. The centre strip says what the list is — repository and kind, or the section name — and carries the open/closed toggle, refresh, and the right panel toggle.

### 3.2 Sidebar — navigation

- **Sections** — four fixed rows, each an icon and a label, with a right-aligned count when it is known (unread for the inbox): Inbox, My pulls, Reviews (review requested), Assigned. Selected = `row.active` fill.
- **Repositories** — a small muted label, then the repositories grouped under their owner, the way the reference groups chats under a project: an owner heading (chevron, the owner's name muted, a count) that folds its group and remembers that it did, and under it one row per repository, indented, the name alone since the heading already says whose, with a lock glyph when private. Owners are ordered by their most recently pushed repository, and the repositories inside the same way. Picking one lists its pulls; the kind toggle is in the centre strip.
- **Footer** — the viewer's avatar at 24 px (their initial until it arrives) and their name, then the appearance control, then the sign-out mark. This is the only place the viewer appears: the sidebar carried the same avatar and login at its head too, and one window does not need to say twice whose it is. No app name and no logo anywhere in the column — a window's title is what it is showing, and the person whose inbox this is says more than the app's name would.
- **The appearance control** — a moon for dark, a sun for light; a click flips to the other. Two states, not three: *follow the system* was a deferral rather than a palette, so choosing it could land on the appearance already showing and look like the control was broken. What it did well survives without being a state anyone has to see — until the reader picks a side, nothing is stored, and the window opens on whatever the OS is showing and follows it. The first click takes them off that, in the direction the icon shows.

### 3.3 Centre — the list, the finder, or a search

The centre strip carries chips for a repository — *Pull requests*, *Issues*, *Files*, *History* — then *Open*/*Closed* for the two lists, then a search box (240 px) that runs GitHub's issue search on ⏎ and lists the answer with no sidebar row highlighted.

**The files column** (the *Files* chip) is a search box over a virtualized list that is one of two things. With the box empty it is the repository's **file tree**: one row per entry at 28 px, indented 13 px a level, a chevron and a folder mark on a directory and a page mark on a file, directories before files and each alphabetical, as GitHub lists them. Everything starts folded; picking a directory folds or unfolds it, picking a file reads it into the right panel. The tree is folded from the paths themselves rather than from GitHub's directory entries, so no folder can appear that holds nothing to open. Opening a file from somewhere else — a launch argument, a link — unfolds the way down to it and marks it.

With anything typed in the box the same list is the **finder**: every path matched fuzzily and case-insensitively, flat, the directory muted and the file name in `text.secondary`, because a match is about the whole path and not about where it sits. Either way, when GitHub cut the tree short the column says so in `status.attention` under the box.

A `uniform_list` of two-line rows at 56 px:

1. State glyph, title (truncated), then for a pull how its checks stand — a green check, a red cross, or an amber dot while they run, the way GitHub's own list marks them — and the right-aligned `#number`. The marks come from one GraphQL query per list (`GitHub::pull_checks`, fifty pulls a query, the head commit's `statusCheckRollup`), not a request per row, and land a moment after the rows do.
2. Author, age, comment count, and up to three labels as small chips coloured from the label's own colour at 22 % over the glass.

An inbox row is the same shape with the reason (`review requested`, `mention`, `subscribed`) where the author would be, and the unread dot. Empty and error states are one muted line each. A *first* load is a skeleton — pulsing bars in the shape of the rows that are coming, in the row tint — and a refresh over a stale list keeps the list and dims nothing; the refresh glyph spins instead. The same holds for the finder, the detail and a diff: each has a skeleton in its own shape.

**The history** (the *History* chip) is the repository's commits, newest first, one 52 px row each: the subject, then the author, the age and the short hash in mono under it. Down the left runs the **rail**, which is what says the history is not a line. A commit's dot sits in its lane, filled for a plain commit and drawn as a ring for a merge. Threads run between the dots: straight down a lane, and **curved** where one changes lane — an S that stands vertically at both of its ends, because a bend meets a straight run of its own thread at the row's edge and meets a dot at the row's middle, and a curve arriving sideways at either leaves a hook. A thread takes the colour of the outer of the two lanes it touches, one colour a lane from the theme's own accent and status hues, so a branch is told from the trunk without a legend. Lanes are claimed as commits appear and freed when nothing is waiting for them, so a straight history stays in one lane and a merge opens exactly one more; the rail stops at six lanes, because a history wider than that is not read by its picture.

The rail is **painted**, not built out of boxes: `paint_path` fills rather than strokes, so a thread is a ribbon — the curve and a copy of it a hair to the right — drawn as a run of small quads, each convex and each filling exactly, which a single long outline does not. A bend into a lane that is already running does not replace that lane's own line; both are drawn, and the join reads as the Y it is. The lanes themselves, and where each thread enters and leaves a row, are worked out in `e1_ui::graph`, away from the window, where they are tested. Picking a commit reads it into the right panel.

### 3.4 Right panel — the item

- **Header** — `#number` and the title at 15/500; under it the state glyph and word, the author with their avatar at 18 px, and for a pull `base ← head`, `+adds −dels`, `n files`. Then the actions: *Close* or *Reopen*, and *Open on GitHub* with the external-link mark. The merge lives in its own card in the conversation, below. While a write is in flight the row says *Working…*; a refusal is GitHub's own words in `status.error` beside the buttons.
- **Facets** — at the top of the conversation, GitHub's own sidebar shape: *Labels*, *Assignees*, *Projects*, each a heading with a gear, then what the item has (label pills in their colours, avatars with logins, project titles; *None* and *assign yourself* otherwise). Each facet is one row — the name at 72 px, the values, the gear — so the three take three lines. The gear opens a popover under its row (floated with `deferred` + `anchored`, so nothing below moves): a filter field, then one row per thing the repository — or, for projects, the owner — offers, with a colour dot or avatar, the name, a description, and a check on the rows the item has. A click adds or removes and the popover stays; a press anywhere outside it, or the gear, closes it. One at a time.
- **Checks and merge card** — for any pull with a head, under the facets: a card in three bands, GitHub's. The checks band is there for a merged or closed pull too — a red run is explained by its log, and the log outlives the merge — and its runs are folded from the start — the summary line says what matters, and the chevron unfolds them — each row carrying two small bordered buttons that do one thing each — *Log* (a terminal glyph, in the accent) opens the job's log in this column, *Open in browser* (an external-link glyph, muted) opens the check on the web; the row itself does nothing on a click, so one cannot fire the other. The conflicts and merge bands appear only while the pull is open. *All checks have passed / n successful checks* with a green badge (red *Some checks were not successful* with the failing and passing counts; amber *haven't completed yet*, its badge turning), unfolding to the runs; *No conflicts with base branch / Merging can be performed automatically* (or the conflict, or *Checking…*); then the button carrying the method — *Merge pull request*, *Squash and merge*, *Rebase and merge* — in white on a deep green (`Colors::merge_button`: the done hue at 32 % lightness, because the status green is made for a glyph on the glass and white did not read on it), both segments 30 px tall, with a chevron segment whose popover lists the three methods with GitHub's descriptions and a check on the current one. The first press turns the button into *Confirm merge* beside *Cancel*; the second merges. A draft or a conflicting pull greys the button. The card's border is green when everything is go and red when a check failed.
- **Body** — markdown at the transcript measure (`TextView::markdown`).
- **Comments** — a hairline, then each comment as avatar (20 px, or the initial in a tinted circle until it arrives), login, age, and its markdown body. The composer is not in the scroll: it sits at the foot of the panel, always in view — a card holding a textarea that grows from two to eight rows and, inside the card, the buttons: *Comment* (filled), and for a pull *Approve* and *Request changes*, which submit a review with the box's words as its body. ⌘⏎ sends a comment.
- **Tabs** — for a pull only: *Conversation* and *Files n*, as chips under the header; on Files, *Unified* / *Split* chips at the row's right. Files is one virtualized list with variable row heights: each file is a 22 px header row on `bg.raised` (fold chevron, a one-letter status mark in the status colours, the path in mono, `+n −m`) followed by its diff — unified, line-numbered both sides in mono at 12 px, or split, the old file on the left and the new on the right with a hairline between and replaced lines paired on one row — added and removed lines tinted by `status.done` and `status.error` at 12 %, hunk headers on `code.bg`. A line longer than its column wraps, in either view, and the row grows with it; nothing is cut off at the edge. Everything starts unfolded; a header folds its file. **Clicking a line** opens a comment box under it (a card naming the line, a textarea, *Cancel* and *Add comment*); the comment lands on that line and side at the pull's head. **⇧-clicking another line** of the same file and side stretches the comment over the lines between the two, which are tinted in the accent at 18 % and named *Lines a–b* on the card; the comment then lands on that range, as on GitHub. A comment on a range says so beside its author. Comments already on the diff hang under their lines as cards with an accent left rule — avatar, login, age, markdown — and ones whose line has since changed hang at the file's end marked *outdated*.
- **A file** — when the files column opened one: the path in mono, the repository, size, line count and what it was read as, then the lines in a virtualized list with numbers in the gutter, **syntax highlighted**. The grammars and highlight queries are the toolkit's — `gpui-component`'s tree-sitter set, which is Zed's — so the colours are the ones its own editor uses and no query file lives here; `e1_ui::code` maps a path to a language name, parses the file once when it lands, and each row asks the parse for the styles on its own line as it is drawn. The palette follows the window's appearance, light or dark, from the same parse. A file with no grammar for its name, or over a megabyte, is drawn plain. A binary or oversized file is one sentence and the *Open on GitHub* control.
- **A commit** — when the history opened one: the subject at 15 px, the message body under it in mono, then the author's picture and name, the age, the short hash on `code.bg` and a *Merge* chip when it has two parents. Under that the file count, `+n −m`, and the same *Unified* / *Split* chips the files tab carries. Then the diff itself, drawn by the very same rows: a pull's files and a commit's files are the same thing to a reader, so they are the same code here, and a commit simply has no comments to hang under its lines.
- **The ask strip** — along the bottom of the column whenever there is something to ask an agent about: an item, lines picked out of a job's log, or text dragged over in a rendered body. It says what would be sent (*3 lines selected*, *18 words*) and the button names where it would go (*Ask Claude Code*), dim until there is something to send. **Dragging over text** in a comment or a body raises a small chip where the pointer let go, which does the same thing: text selection is a window-wide affair in the toolkit, so the column asks for it on mouse-up and remembers what came back.
- **The ask itself is a dialog**, not a panel. What is being sent is worth reading before it goes: the line to type in, then the excerpt under the words that say where it came from, then **what goes with it** — the repository and owner, the issue or pull number, the branch and head commit, the labels; for a log the job and workflow run ids, the step that failed and the pull the log was opened from — and then the CLIs found on this machine, the default marked and tinted. Picking one sends and closes; ⏎ sends to the marked one. Picking a different one makes it the new default, which the settings remember. The dialog layer is rendered by the window (`Root::render_dialog_layer`), which the toolkit's `Root` does not do on its own.
- **A job's log** — when a run's *Log* was pressed: a *‹ Back* link to whatever was on screen before, the job's name, the repository and job number, then the log grouped the way GitHub's job page groups it. Each of the job's steps (from the Actions API, not the text) is a foldable row: a chevron, how it went (a green check, a red cross, an amber spinner turning while it runs, a muted ring for a step that was skipped), its name, and at the right how long it took in the mono face. A failed step is named in red on an 8 % red tint. The failed and still-running steps start unfolded, the rest folded, until the reader folds or unfolds one themself. Under an unfolded step sit the lines the runner wrote while that step ran — the runner does not say which step a line belongs to, so `e1_ui::log::assign` places each line by its clock against the steps' start times. A line shows the runner's clock (`HH:MM:SS`) in the gutter in place of a number and wraps when it is longer than the column: the list is gpui's variable-height `list`, as the diff is. The runner's markers become colour and weight rather than text: `##[group]` a bold heading on `bg.raised`, `##[command]` in the accent, `##[error]` and `##[warning]` in their status colours on a 12 % tint, `##[endgroup]` dropped. A job that reports no steps shows its lines whole under a one-line note. Only GitHub Actions jobs have a log the API will hand over; other checks have only *Open in browser*. The window's own open-on-GitHub control opens the job's page while a log is on screen.
- **Markdown** — links in the accent, table heads as a translucent dark band (`black` at 35 %) over the glass with `text.secondary`, rows separated by `border.subtle`, inline code on `bg.surface`.
- **Footer** — *Open on GitHub*, in the right strip, which opens the browser; every item is one click from the real thing.

Empty state: *Pick something to read* over the glass.

### 3.5 Sign-in screen

What the centre column is when there is no token: the logo at 56 px, *Sign in to GitHub*, one sentence on what the app reads, and one filled button. Pressing it swaps the button for the device code in mono at 24 px inside a card, the address to enter it at, *Open in browser* (which also copies the code) and *Copy code*, and a quiet *Waiting for GitHub…* line. A refusal or an expiry is one sentence in `status.error` with *Try again*. Signing out is the small mark beside the login in the sidebar footer.

## 4. Component mapping

| Region | Component |
| --- | --- |
| Window shell | `gpui-component` `Root`, our header strips |
| Columns | ours: three flex children with explicit widths, a 9 px grab area centred on each divider, and the drag tracked at the window root |
| List | `gpui::uniform_list` with rows from `e1_ui::rows::ItemRow` |
| Markdown | `gpui-component` `TextView::markdown` |
| Tooltips, icons | `gpui-component` primitives; our SVGs in `assets/icons/` for what the toolkit lacks (pull request, merge, issue, comment, lock) |

## 5. Interaction rules

- **Anything still running turns — and is asked about again.** A check in progress, a step still running, the checks card's own badge while runs are pending, and the sign-in screen's wait all use the toolkit's `Spinner` on the loader glyph. A static loader glyph reads as broken. GitHub does not push the end of a run, so the detail column re-asks every 20 s about whatever on screen is still pending — the checks on the pull, or the steps and log of the job — and the spinner stops when the work does.
- `⌘B` sidebar, `⌘⌥B` right panel, `⌘R` refresh what is on screen. `⌘K` lands in M3.
- Picking a row opens it on the right and never navigates the centre away.
- Never block: every fetch shows the stale value until the fresh one lands.
- Truncate repository names and titles from the left only when the tail is the meaningful part (repository names); titles truncate from the right.

## 8. The logo

`assets/icons/e1.svg`: an uppercase `E` and a `1` in strokes at 2.2 on the 24-grid, one colour. It is painted in `Tokens::logo()` — white on the dark theme, navy (`#1E1B4B`) on the light one — which is not a token because no other part of the window uses it and a theme file should not have to name the logo. It sits on the sign-in screen at 56 px, and nowhere else in the window.
