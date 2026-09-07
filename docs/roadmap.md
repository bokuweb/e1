# e1 Roadmap

> Status: **M0 and M1 landed; M2 in progress**.
> Last updated: 2026-09-05

## 1. Vision

**e1 is a native GitHub client, written in Rust on [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), built to stand on its own and to be embedded in [Ginka](https://github.com/bokuweb/ginka).**

Ginka is an orchestrator for coding agents. The work those agents produce ends up on GitHub as pull requests, and the work they are asked to do starts there as issues and review requests. Today that half of the loop lives in a browser tab. e1 is that tab as a native surface: the inbox, the pull requests waiting on you, the issues assigned to you, and any repository's open work, in a window that looks and behaves like Ginka's — so that, once it works on its own, the same views can be mounted inside Ginka's right panel.

Three properties drive every decision below:

1. **Same bones as Ginka.** The same toolkit at the same rev, the same token schema, the same three-column frameless window, the same crate split. Sameness is not aesthetic here; it is what makes embedding a mount rather than a port.
2. **Standalone first.** e1 must be a useful GitHub client on its own, with no Ginka process anywhere. Embedding is a later milestone that must not be paid for up front in complexity, only in discipline (§4.3).
3. **Local-first, token-only.** A GitHub token is the only requirement. It is found in the environment or in `gh`, or obtained by signing in from the window and kept in the platform keychain; it never touches a plain file, and nothing needs an account beyond that.

## 2. What we take from the references

- **Ginka** (`bokuweb/ginka`) — the window: frameless, glass over a blurred desktop, three resizable columns with per-column header strips that drag the window. The crate split (`*-ui` holds what is testable without a window; the binary holds only views — here, the views move to a library). The tokens, verbatim in schema. The conventions: rustdoc on every public item, test-first for decisions, en+ja from the first commit.
- **GitHub's own web client** — what a person expects to find: notifications grouped by repository with the reason, the pull request header (base ← head, +/-, checks, review decision), the issue timeline. Read for what to show; the rendering is ours.
- **`gh` CLI** — the token, and the search syntax (`is:pr review-requested:@me`) that names the fixed sidebar sections.

## 3. Scope

### 3.1 v1.0 definition of done

A user can: open the window → see the unread inbox, the pull requests they authored, the ones waiting for their review and the issues assigned to them → pick a repository and list its open pull requests or issues, open or closed → open any item and read its description and comments → open it on GitHub → and have the arrangement of the window survive a restart. Then, in Ginka: the same views mounted as a surface, drawing through Ginka's daemon.

### 3.2 Non-goals for v1

- Being a general-purpose git client. Local repositories are Ginka's business.
- Writing to GitHub before reading it is right. Marking read, commenting and reviewing land in M2 and M3, after the read path is trusted.
- GitHub Enterprise Server, multiple accounts. One token, `api.github.com`.
- Its own offline database. Two caches — answers with their `ETag`s, and a snapshot of the store's memory — are what makes a launch instant and a refresh cheap (§4.8); a queryable database waits until something needs a query.

## 4. Architecture

### 4.1 Process model

One process. There is no daemon: GitHub is the remote, and the token is the only state, and it is not ours. The window holds view state and in-memory caches only; closing it loses nothing but a few seconds of fetching.

```
┌────────────────────────────────────────────┐          ┌────────────────┐
│  e1 (GPUI app)                             │  HTTPS   │  api.github.com│
│  src/main.rs   opens the window            │◄────────►│                │
│  e1-views      Shell / Sidebar / List /    │  (ureq,  │                │
│                Detail, over Arc<dyn GitHub>│   bg exec)│               │
│  e1-ui         tokens, settings, view models│         └────────────────┘
│  e1-github     GitHub trait, REST, Scripted│
└────────────────────────────────────────────┘
```

When embedded (§4.3), the same `e1-views` sits inside Ginka's window and the `Arc<dyn GitHub>` it is given proxies through Ginka's daemon, so Ginka's rule that the daemon owns all state holds without the views knowing.

### 4.2 Crate layout

```
e1/
├─ Cargo.toml              # workspace root; the `e1` binary, deliberately thin
├─ src/main.rs             # paths, settings, locale, logging, the window, Shell
├─ crates/
│  ├─ e1-github/           # model, GitHub trait, REST client, token discovery,
│  │                       # Scripted fake. No GPUI.
│  ├─ e1-ui/               # Tokens + theme apply, Assets, Layout, AppSettings,
│  │                       # Paths, i18n, logging, nav and row view models, Fetch
│  └─ e1-views/            # Store, Shell, Sidebar, ItemList, Detail. A library,
│                          # with no tests (AGENTS.md rule 6).
├─ locales/app.yml
├─ assets/themes/{dark,light}.json
├─ assets/icons/*.svg
└─ docs/{roadmap,ui}.md
```

Dependency direction: `e1-views → e1-ui → e1-github`. `e1-github` knows nothing about the UI; `e1-ui` knows the model but not the views; `e1-views` is the only crate that touches `gpui-component`'s render chains.

### 4.3 The embedding contract

What Ginka will do, in its M-later "GitHub surface", is add `e1-views` to its workspace and mount `e1_views::GitHubPanel` (the list and the detail without the sidebar) into its right panel, and `e1_views::Sidebar`'s sections into its own sidebar. For that to be a mount, five things are held constant from now on:

| # | Constraint | Why it is decided now |
| --- | --- | --- |
| E1 | **Views are a library crate.** `src/main.rs` opens a window and nothing more. | A view in a binary cannot be linked. |
| E2 | **`Arc<dyn GitHub>` is the only way a view reaches the network.** The trait is blocking, `Send + Sync`, and called on the background executor. | Ginka's daemon owns state; its implementation will answer over its RPC. A blocking trait can be implemented over a WebSocket channel with `block_on`; an async trait would fix the executor. |
| E3 | **No second reactor.** `ureq` over rustls, no tokio anywhere in the graph. | Ginka runs on `smol` and refuses another runtime in its process. |
| E4 | **`gpui-component` and `gpui` at Ginka's locked revs**, and no other GPUI library. | Two revs of `gpui` are two unrelated `App`, `Window`, `Element` types. `Cargo.lock` was seeded from Ginka's for this reason, and a toolkit bump here follows one there. |
| E5 | **The token schema is Ginka's.** `e1_ui::Tokens` deserialises the same JSON; when embedded, the host's tokens are installed instead of ours. | A view written against `text.secondary` renders correctly under either app's theme. Extracting a shared `glass-tokens` crate is the M4 step that makes this a type rather than a convention. |

What is *not* held constant: the sidebar's own header, window controls and settings persistence, which are the standalone shell's and will be the host's when embedded. They live in `Shell`, and `Shell` is the one view Ginka will not mount.

### 4.4 Data model

Everything a view draws is one of these, in `e1_github::model`:

| Type | What it is | Where it comes from |
| --- | --- | --- |
| `RepoId` | `owner/name`, the key everything else hangs off | parsed from `full_name`, `repository_url`, or a subject URL |
| `Repo` | a repository the viewer can reach: description, private, default branch, stars, last push | `GET /user/repos?sort=pushed` |
| `Viewer` | who the token is | `GET /user` |
| `Notification` | one inbox row: subject title and kind, the reason, unread, the repo, the item number when there is one | `GET /notifications` |
| `Item` | a pull request *or* an issue as a list row and a detail header: number, title, `Kind` (issue, or pull with draft/merged), open/closed, author, timestamps, labels, comment count, body | `/pulls`, `/issues`, `/search/issues`, `/issues/{n}` |
| `Pull` | an `Item` plus what only a pull has: base and head, additions, deletions, changed files, mergeability | `GET /repos/{r}/pulls/{n}` |
| `Comment` | one timeline entry: author, time, markdown body | `GET /repos/{r}/issues/{n}/comments` |
| `Checks` | every check run and commit status on a commit, with a tally and an overall state; an Actions job's run carries its id so its log can be read | `GET /repos/{r}/commits/{sha}/check-runs` and `…/status` |
| `ReviewComment` | a comment on a line of a pull's diff: path, line and side, or no line when the code has since changed | `GET`/`POST /repos/{r}/pulls/{n}/comments` |
| a job's log | plain text, following GitHub's redirect | `GET /repos/{r}/actions/jobs/{id}/logs` |
| `PullFile` | one file of a pull's diff: path, status, counts, the unified patch when GitHub sends one | `GET /repos/{r}/pulls/{n}/files` |
| `Tree` | every path in a repository at its default branch, and whether GitHub cut the list short | `GET /repos/{r}/git/trees/HEAD?recursive=1` |
| `FileContent` | one file: size, the text when it is text and under GitHub's inline limit, the web URL otherwise | `GET /repos/{r}/contents/{path}` |

One `Item` type for both pulls and issues, rather than two, because every list and every detail header draws them the same way and only the state glyph differs; `Kind` is where that difference lives. A merged pull is `closed` on the wire with `merged_at` set — the wire never says "merged" — so `Item::state()` is where that rule is written and tested.

### 4.5 The `GitHub` trait

```rust
pub trait GitHub: Send + Sync {
    fn viewer(&self) -> Result<Viewer>;
    fn notifications(&self) -> Result<Vec<Notification>>;
    fn repositories(&self) -> Result<Vec<Repo>>;
    fn items(&self, repo: &RepoId, kind: ListKind, status: StatusFilter) -> Result<Vec<Item>>;
    fn search(&self, query: &str) -> Result<Vec<Item>>;
    fn item(&self, repo: &RepoId, number: u64) -> Result<Item>;
    fn pull(&self, repo: &RepoId, number: u64) -> Result<Pull>;
    fn comments(&self, repo: &RepoId, number: u64) -> Result<Vec<Comment>>;
    fn pull_files(&self, repo: &RepoId, number: u64) -> Result<Vec<PullFile>>; // defaults to Unsupported
    fn tree(&self, repo: &RepoId) -> Result<Tree>;                               // defaults to Unsupported
    fn file(&self, repo: &RepoId, path: &str) -> Result<FileContent>;            // defaults to Unsupported
    fn avatar(&self, url: &str) -> Result<Vec<u8>>;                              // defaults to Unsupported
    fn comment_on(&self, repo: &RepoId, number: u64, body: &str) -> Result<Comment>;
    fn set_open(&self, repo: &RepoId, number: u64, open: bool) -> Result<Item>;
    fn merge(&self, repo: &RepoId, number: u64) -> Result<()>;                   // all three default to Unsupported
}
```

A write goes through the same trait as a read, and after one lands the store reads the item again rather than patching what it has: the write's answer is partial (a comment, a state) and GitHub's is the whole, which the `ETag` cache makes cheap to ask for.

Small on purpose: every method is one screen's question. Write operations arrive in M2/M3 as new methods with a default `Err(Unsupported)`, so a host implementation that cannot do them yet still compiles.

Two implementations ship: `Rest` (ureq, pagination by `Link` header up to a page cap, rate-limit headers surfaced as a typed error) and `Scripted` (in-memory, with a sample data set used by tests and by `E1_DEMO=1`).

### 4.6 Signing in

Three ways in, in order of authority: the environment (`E1_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`), the keychain entry this app wrote, and `gh auth token`. With none of them the window opens on a sign-in screen that runs GitHub's **device flow**: the app asks GitHub for a short code, shows it, opens `github.com/login/device` in the reader's own browser with the code on the clipboard, and polls until GitHub hands it a token. No browser is embedded and no password passes through the process. The token goes to the macOS keychain through the `security` command (`security -i` reads from stdin, so it never appears in a process listing); signing out deletes that entry and only that entry — a token from the environment or from `gh` is someone else's to remove.

The device flow needs an OAuth app with the flow enabled. e1 ships as one — `bokuweb`'s `e1` app, whose client id is in the source (`auth::DEFAULT_CLIENT_ID`), because a client id is public by design and the device flow has no secret. A fork that registers its own sets `E1_GITHUB_CLIENT_ID` at build time or at run time.

### 4.7 Caches

Two, at two levels, and neither is a database.

- **Answers with their tags** (`e1_github::HttpCache`, `~/.e1/cache/http/`). Every `2xx` the REST client sees is kept under its URL with the `ETag` GitHub sent and the `next` page if there was one; the next request for the same URL carries `If-None-Match`, and a `304` is answered from disk. GitHub does not charge a `304` against the rate limit, so a refresh of the whole window costs round trips and nothing else. The cache knows nothing about what a body means, so it is right for every endpoint at once.
- **The store's memory** (`e1_ui::snapshot`, `~/.e1/cache/store.json`). After every answer lands, the store writes what it knows — viewer, repositories, inbox, every list, the last forty items read — and the next launch reads it before the first request goes out. The window opens on yesterday's inbox and replaces it a moment later, rather than opening on *Loading…*.

Both are cleared on sign-out, because a `304` for the last account's inbox is the last account's inbox. Both are safe to delete at any time.

Avatars are a third, simpler one: GPUI draws an image from a path and this app has no HTTP client the window could hand it, so the store fetches each avatar once through the trait (at 80 px) and keeps it under `~/.e1/cache/avatars/` named by a hash of its URL. Until it is there, the initial in a tinted circle stands in.

### 4.8 UI stack

`gpui-component` at rev `5a564d4` over `gpui` at zed rev `ef07591`, exactly Ginka's lock. The reasoning is Ginka's (`docs/roadmap.md` §4.6 there) and is not repeated; the additional constraint here is E4. Used from it: `h_resizable`/`resizable_panel`, `Root`, `Icon`, `TextView::markdown`, `Tooltip`, `Input`. Built here: the frameless header strips, the state glyphs, the list rows, the detail header.

## 5. Milestones

| Milestone | Delivers | Status |
| --- | --- | --- |
| **M0 Shell** | Workspace mirroring Ginka's; tokens, assets, settings, layout persistence; frameless glass window with three resizable columns and draggable header strips; `⌘B`/`⌘⌥B`; en+ja; token discovery; `Scripted` and `E1_DEMO=1` | landed |
| **M1 Read** | Inbox; the four fixed sections (inbox, my pulls, review requests, assigned); repositories; per-repo pulls and issues, open/closed; detail with markdown body, labels, pull header, comments; open on GitHub; `⌘R` refresh; stale-while-revalidate `Fetch` | landed |
| **M2 Review** | Pull files and diffs (landed: a Files tab, every diff in one virtualized list, folded per file, unified or split, `e1_ui::diff`); comments on diff lines, read and written (landed); sign in from the window by device flow, token in the keychain, sign out (landed); the file finder and file reading (landed); search over issues and pulls (landed); the two caches (landed, §4.7); avatars (landed); checks and their Actions logs (landed); review decision; mark a notification read; polling the inbox | in progress |
| **M3 Act** | Comment, close, reopen, merge with a method (landed); approve / request changes (landed); labels, assignees and projects edited in place (landed — projects over GraphQL, which needs the `project` scope); the checks and the merge as GitHub's card (landed); draft and ready for review (landed, GraphQL); edit title and body; `⌘K` palette over every action and repository | in progress |
| **M4 Embed** | Extract the shared token crate (E5 as a type); `GitHubPanel` mounted in Ginka's right panel over a daemon-backed `GitHub`; Ginka's sidebar shows the sections | |
| **M5 Polish** | Light theme sign-off, keyboard traversal audit, reduce-motion, virtualized detail timeline, on-disk cache if the in-memory one proves too little | |

## 6. Quality bars

- **Performance.** A list of a thousand rows scrolls at frame rate: `uniform_list`, rows built from precomputed `ItemRow`s, no per-frame formatting.
- **Never block the UI thread.** Every trait call runs on the background executor; a stale value is shown while the fresh one loads.
- **Accessibility.** AA contrast pairs in both themes; every state has an icon as well as a colour; every control keyboard-reachable; 28 px minimum hit target.
- **i18n.** `en` and `ja` maintained together in `locales/app.yml`; a missing key is a review failure, not a runtime one.

## 7. Open questions

| # | Question | Notes |
| --- | --- | --- |
| Q1 | Offline cache on disk? | Not until the in-memory `Fetch` is shown to be too little. If it lands, SQLite, and behind the trait so the host's daemon can own it. |
| Q2 | GraphQL for the pull header? | Review decision and checks are one GraphQL query and three REST calls. Decide in M2 with the numbers. |
| Q3 | Licence | Decide before the first public commit. Nothing GPL is linked. |

## 8. Decision log

| Date | Decision | Why |
| --- | --- | --- |
| 2026-09-05 | Views live in `e1-views`, a library, not in the binary | Embedding (E1). Ginka keeps views in its binary because nothing mounts them; here something will. |
| 2026-09-05 | The `GitHub` trait is blocking, run on the background executor | A host that owns state behind a socket can implement a blocking call with `block_on`; an async trait would commit both apps to one executor (E2, E3). |
| 2026-09-05 | `ureq` for HTTP, no tokio | E3. Every async client on crates.io brings tokio; Ginka runs on smol. |
| 2026-09-05 | `Cargo.lock` seeded from Ginka's | E4. The toolkit's own manifest does not pin `gpui`, so a fresh resolve would take zed's HEAD and diverge from what `gpui-component 5a564d4` was built against. |
| 2026-09-05 | Token schema copied from Ginka; no shared crate yet | E5 as a convention now, a type in M4. A shared crate before there is a second consumer is a crate with one user. |
| 2026-09-05 | One `Item` type for pulls and issues | Every list and detail header draws both the same way; the state glyph is the only difference and `Kind` carries it. |
| 2026-09-05 | Token discovered, never stored | Rule 8. `gh` already keeps it in the keyring; a second copy on disk is a second thing to leak. |
| 2026-09-05 | Sign in by device flow; the token goes to the keychain through `security`, not to a file | Superseding the line above for the token the window obtains: a client that cannot sign itself in is one that only works for people who already have `gh`. The keychain is where `gh` keeps its own, and `security -i` keeps the secret off the command line. Shelling out rather than linking Security.framework keeps the crate free of a platform dependency it would use in one place. |
| 2026-09-05 | A pull's diff is one file at a time, not all at once | Superseded the next day: see below. |
| 2026-09-06 | A pull's diffs are one virtualized list across every file, each foldable | With the rows in a `uniform_list` a hundred files cost what the screen shows, so the reason to open one at a time went away, and a review reads top to bottom. The file headers are rows of the same height as the lines, which is what lets it be one list. |
| 2026-09-06 | Answers cached by `ETag`, and the store snapshotted, rather than a local database | Both are dumb and both are enough: a `304` is free and a snapshot makes the first frame full. A database earns its schema when something needs a query across what was fetched, and nothing does yet. |
| 2026-09-06 | Merge takes two presses; close and comment take one | A merge is the one action here git cannot take back, so the button arms on the first press and says *Merge now?*; a close can be undone with the button beside it, and a comment can be deleted on the web. A modal would be the alternative, and `docs/ui.md` §6 has no modals but destructive confirmations — this is that confirmation, in place. |
| 2026-09-06 | The logo is one colour — white on dark, navy on light — drawn with `Icon` | Superseding the gradient mark of the same morning: a coloured square read as a badge rather than a mark, and one colour that answers the theme is what every other glyph in the window does. The colour is a method on `Tokens`, not a token, because no theme file should have to name the logo. |
| 2026-09-06 | The `dev` profile optimises dependencies | A debug GPUI drops frames scrolling fifty rows, and a window that stutters cannot be judged. Dependencies rarely change, so their optimisation is paid once; our crates stay at `opt-level = 1` to keep the edit loop short. |
| 2026-09-06 | The sidebar is frosted, not painted | A second coat of dark over the window composited to near-opaque and the glass was lost on the left. A white tint at 6 % over a window at 72 % is what a frosted pane looks like, and it separates the column from the centre by tone rather than by darkness. |
| 2026-09-07 | The checks stay on a merged pull, with their runs unfolded | The card was gated on the pull being open, so the moment a pull merged its logs went with it — and a merged pull with a red run is exactly when someone goes looking for the log. The runs are unfolded from the start because the Log link inside a folded row was not being found. |
| 2026-09-07 | The diff is a variable-height `list`, no longer a `uniform_list` | A comment under a line is taller than a line, and so is the box for writing one. gpui's `list` measures what it draws and stays virtualized; the rows keep their fixed heights where they had them. |
| 2026-09-07 | A split diff pairs a removal with the addition that follows it | Side by side is only worth its width when a changed line reads as one row; pairing the two runs in order is what GitHub does and what a reader expects. The leftovers of the longer run stand alone with an empty half. |
| 2026-09-07 | Every column's view sits in a `flex_1 min_h_0` slot under its strip | A view given the column's full height under a 44 px strip ran 44 px past the window, and that 44 px was where the comment button and the sidebar footer lived. The same floor that let the conversation scroll shrink lets each column's view fit. |
| 2026-09-07 | Pickers and the merge menu are popovers, not inline | Opening one inline pushed the conversation down and widened the card; a `deferred` + `anchored` layer over the page moves nothing, and a press outside closes it. Where GitHub's shape is a popover, ours is one too now. |
| 2026-09-07 | A log is grouped by the job's steps, placed by the clock | GitHub's job page folds the log into steps, and that is how a reader finds the failing one. The log text does not say which step a line is from, but the Actions API says when each step started; `e1_ui::log::assign` puts each line under the last step that had started when the line was written. The steps are one more `GitHub::job` call, cached like the log. |
| 2026-09-07 | The log is a variable-height `list`, and its lines wrap | A `uniform_list` cut long lines off at the column's edge, and a build log is mostly long lines. The same `list` the diff moved to measures each row. |
| 2026-09-07 | A run's row has two buttons, not a click | The whole row opened the log, and the *Details* link on it opened the web — and, being inside the row, both. *Log* and *Open in browser* are now two buttons that each do one thing, and the row does nothing. A log remembers what was on screen before it, so *Back* returns there. |
| 2026-09-07 | The checks card starts folded again | It was unfolded because the *Log* link inside a folded row was not being found. Now that each run has two visible buttons, there is nothing to hunt for, and a card of a dozen runs pushed the merge button off the screen. |
| 2026-09-07 | A pending check turns | The loader glyph standing still looked like a failure of a different kind. The toolkit's `Spinner` animates it; the same one now stands wherever the app is waiting on something. |
| 2026-09-06 | The editable parts borrow GitHub's shapes | A reader who knows GitHub's gear-and-filter picker and its three-band merge card is not asked to learn ours. The first attempt (chips under a row, a *Merge now?* toggle) was smaller and read as a puzzle. Where GitHub's shape is a popover, ours opens in place under the heading: the panel is narrow and a popover over it would cover what it edits. |
| 2026-09-06 | Projects go over GraphQL; everything else stays on REST | Projects (v2) have no REST surface. One `graphql` helper carries the four queries; the `ETag` cache does not apply to them, which is fine for a picker. The device-flow scope grows `project`; a `gh` token without it gets GitHub's refusal in the picker rather than a silent empty list. |
| 2026-09-06 | A divider drag is tracked at the window while it is held | An element's mouse-move listener only hears the pointer while it is the hovered one, and a drag across the centre crosses text fields and scrollbars that claim the pointer. A `canvas` registered for the drag's duration hears every move. |
| 2026-09-06 | The columns are ours, not the toolkit's resizable group | Two rounds of reading the toolkit's resize algorithm left the right divider still not dragging, and a divider is not a place to keep guessing. Three flex children with explicit widths, a grab area on each edge and the drag tracked at the root is forty lines, all of them ours to debug; it also made the slide animation and the centre's floor trivial. `flex_none` (below) was the previous attempt. |
| 2026-09-06 | Loading is a skeleton, not a word | A reader who sees the shape of a list knows a list is coming and where to look; *Loading…* says only that they are waiting. Only a first load shows one — a refresh keeps the stale value. |
| 2026-09-06 | The sized columns are `flex_none` | The toolkit gives every panel `flex_grow: 1`, so the window's spare width was split three ways and dragging the divider on one side moved the column on the other. Only the centre grows now; the toolkit's own docs call this the load-bearing override. |
| 2026-09-06 | Appearance is a three-way cycle in the sidebar footer | Dark, light, follow the system: three states need no menu, and the footer is where the person is. Following the system re-resolves on the window's appearance observer, so a desktop that turns dark at sunset takes the window with it. |
| 2026-09-06 | Repositories are grouped under their owner | The reference sidebar groups chats under a project; a person with three organisations reads their repositories the same way. Folded owners are remembered in `app.json`. |
| 2026-09-06 | Header strips keep 6 px from their column's edges | The resize handle's grab area overlaps the strips, and a press on it meant for the divider was starting a window move. Insetting the strips is what the toolkit's own handle padding assumes. |
| 2026-09-06 | The file finder fetches the whole tree in one request and matches locally | One request for twenty thousand paths and then no latency at all beats a request per keystroke. GitHub truncates very large trees and the finder says so. |
| 2026-09-06 | The OAuth client id is committed | It is public by design: it names the app and authenticates nothing, and the device flow never sees a secret. Keeping it out of the source would only mean every user registering their own app before the sign-in button worked. |
| 2026-09-05 | `pull_files` has a default `Unsupported` body on the trait | The first method added after the trait shipped, and the pattern for every later one: a host implementation that lags the trait still compiles and the view draws the refusal. |
