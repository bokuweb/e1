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
- Its own offline database. Stale-while-revalidate in memory is enough until it is not (§7).

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
| `PullFile` | one file of a pull's diff: path, status, counts, the unified patch when GitHub sends one | `GET /repos/{r}/pulls/{n}/files` |

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
}
```

Small on purpose: every method is one screen's question. Write operations arrive in M2/M3 as new methods with a default `Err(Unsupported)`, so a host implementation that cannot do them yet still compiles.

Two implementations ship: `Rest` (ureq, pagination by `Link` header up to a page cap, rate-limit headers surfaced as a typed error) and `Scripted` (in-memory, with a sample data set used by tests and by `E1_DEMO=1`).

### 4.6 Signing in

Three ways in, in order of authority: the environment (`E1_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`), the keychain entry this app wrote, and `gh auth token`. With none of them the window opens on a sign-in screen that runs GitHub's **device flow**: the app asks GitHub for a short code, shows it, opens `github.com/login/device` in the reader's own browser with the code on the clipboard, and polls until GitHub hands it a token. No browser is embedded and no password passes through the process. The token goes to the macOS keychain through the `security` command (`security -i` reads from stdin, so it never appears in a process listing); signing out deletes that entry and only that entry — a token from the environment or from `gh` is someone else's to remove.

The device flow needs an OAuth app with the flow enabled. e1 ships as one — `bokuweb`'s `e1` app, whose client id is in the source (`auth::DEFAULT_CLIENT_ID`), because a client id is public by design and the device flow has no secret. A fork that registers its own sets `E1_GITHUB_CLIENT_ID` at build time or at run time.

### 4.7 UI stack

`gpui-component` at rev `5a564d4` over `gpui` at zed rev `ef07591`, exactly Ginka's lock. The reasoning is Ginka's (`docs/roadmap.md` §4.6 there) and is not repeated; the additional constraint here is E4. Used from it: `h_resizable`/`resizable_panel`, `Root`, `Icon`, `TextView::markdown`, `Tooltip`, `Input`. Built here: the frameless header strips, the state glyphs, the list rows, the detail header.

## 5. Milestones

| Milestone | Delivers | Status |
| --- | --- | --- |
| **M0 Shell** | Workspace mirroring Ginka's; tokens, assets, settings, layout persistence; frameless glass window with three resizable columns and draggable header strips; `⌘B`/`⌘⌥B`; en+ja; token discovery; `Scripted` and `E1_DEMO=1` | landed |
| **M1 Read** | Inbox; the four fixed sections (inbox, my pulls, review requests, assigned); repositories; per-repo pulls and issues, open/closed; detail with markdown body, labels, pull header, comments; open on GitHub; `⌘R` refresh; stale-while-revalidate `Fetch` | landed |
| **M2 Review** | Pull files and diffs (landed: a Files tab, one diff open at a time, `e1_ui::diff`); sign in from the window by device flow, token in the keychain, sign out (landed); checks summary, review decision, review comments; mark a notification read; polling the inbox | in progress |
| **M3 Act** | Comment, approve / request changes, merge; assign, label; `⌘K` palette over every action and repository | |
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
| 2026-09-05 | A pull's diff is one file at a time, not all at once | Nine diffs stacked in a 420 px column is a wall, not a review; and the panel is not virtualized yet, so one open file is also what keeps a large pull from costing thousands of elements (rule 7 owes this a `uniform_list` in M5). |
| 2026-09-06 | The OAuth client id is committed | It is public by design: it names the app and authenticates nothing, and the device flow never sees a secret. Keeping it out of the source would only mean every user registering their own app before the sign-in button worked. |
| 2026-09-05 | `pull_files` has a default `Unsupported` body on the trait | The first method added after the trait shipped, and the pattern for every later one: a host implementation that lags the trait still compiles and the view draws the refusal. |
