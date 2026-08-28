# Command Routing Architecture

**Status:** normative v0.1 design  
**Scope:** invocation parsing, command ownership, Git compatibility, and process semantics

## Context

JJK has three command classes:

1. **JJK-native** commands express semantic state operations that Git does not have.
2. **Enhanced Git-compatible** commands deliberately own a Git spelling while preserving the underlying Git contract and adding JJK value.
3. **Byte-transparent Git passthrough** sends every other invocation to the real Git executable without JJK initialization, reconciliation, locking, output capture, or argument rewriting.

`jjk status` is an intentional enhancement. `jjk rebase`, `jjk clone`, aliases, external `git-*` helpers, and Git commands introduced after this release are Git. They behave as though the executable name were `git`, not `jjk`.

The old prototype's unknown-word-as-state behavior is incompatible with this promise: `jjk rebsae` must produce Git's normal unknown-command result, never create a state.

The cross-layer mutation protocol is:

> `discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

It applies to JJK mutations and enhanced commands that mutate JJK. It **never runs before transparent passthrough**. External Git mutations reconcile as observed facts at the next semantic JJK operation.

## Decisions

### CR-D01 — Closed, inspectable ownership registry

| Class | v0.1 commands | Contract |
|---|---|---|
| JJK-native | `setup`, `save`, `step`, `nice`, `see`, `return`, `pick`, `fork`, `freeze`, `current`, `story`, `back`, `forward`, `up`, `down`, `archive`, `recover`, `undo`, `redo`, `backup`, `load`, `handoff`, `validate`, `doctor`, `completion`, `help`, `version` | JJK grammar; mutations use the cross-layer protocol |
| Enhanced Git-compatible | `status` | Git status remains the base; JJK owns only its explicit orientation forms described by CR-D08 |
| Explicit Git escape | `git -- <git-argv...>` | Strip exactly `git --`, then invoke real Git with the remaining arguments unchanged |
| Transparent passthrough | every other invocation, including `init` and unknown text | Replace only executable `jjk` with verified real `git` |

New JJK-native names should not overlap Git built-ins. A collision requires an explicit `EnhancedGit` registry decision and compatibility suite. Registered JJK commands win over user-defined Git aliases; a colliding alias remains reachable through `jjk git -- save ...`.

`status` follows CR-D08. No other Git spelling is enhanced in v0.1. In particular, `init`, `branch`, `checkout`, `clone`, `diff`, `log`, `rebase`, `restore`, `show`, `stash`, `worktree`, `fetch`, `pull`, and `push` are transparent Git. Safe-space initialization is the non-colliding native command `jjk setup [path]`; ordinary `jjk init` stays byte-transparent Git.

### CR-D02 — Routing is syntax-only

Routing depends only on invocation syntax and the compiled registry—not repository presence, `.jjk` health, alias/helper discovery, network access, command execution, locale decoding, fuzzy matching, or abbreviation. Unknown verbs go to Git, the authority for built-ins, aliases, helpers, suggestions, and errors.

### CR-D03 — Preserve raw arguments

Capture `std::env::args_os()` as `Vec<OsString>`. Never join, quote, normalize, lowercase, decode, or reconstruct passthrough arguments. On Unix arbitrary non-NUL argument bytes remain bytes; on Windows preserve `OsString`/WTF-16 and platform-native process quoting. The parser returns spans into the original vector; execution consumes the originals.

### CR-D04 — Parse only enough Git global grammar to find an owned verb

Recognize registered commands after known Git globals, so `jjk -C ../repo status` is enhanced and `jjk -C ../repo save -- checkpoint` is native.

Recognized prefix grammar:

- value next or attached where Git accepts it: `-C`, `-c`;
- value next or `=` form: `--git-dir`, `--work-tree`, `--namespace`, `--super-prefix`, `--config-env`;
- no value: `-p`, `--paginate`, `-P`, `--no-pager`, `--no-replace-objects`, `--bare`, pathspec globals;
- terminal/query globals: `--exec-path[=<path>]`, `--html-path`, `--man-path`, `--info-path`, `--version`, `-v`, `--help`, `-h`.

Missing values or unknown leading options make the scan inconclusive and send the **entire original invocation** to Git. This is the forward-compatibility rule for future Git globals. `--` is not a Git global command delimiter: `jjk -- status` passes as `git -- status`, including Git's native error. Once a verb is found, every later `--` belongs to that command. Non-UTF-8 verbs cannot match the ASCII registry and pass through.

For owned commands, recognized globals become a typed `GitContext`; underlying Git adapter calls receive the same original globals. Git probes resolve `-C`, git-dir, and work-tree semantics rather than JJK reinterpreting repository layout.

### CR-D05 — Concrete precedence algorithm

```text
route(argv):
  1. argv begins exactly ["git", "--"]
       => GitPassthrough(argv[2..], ExplicitEscape)
  2. argv is empty
       => JjkNative(Overview)
  3. argv is exactly ["--help"] or ["-h"]
       => JjkNative(HelpIndex)
     argv is exactly ["--version"], ["-v"], or ["version"]
       => JjkNative(Version)
  4. scan known Git-global prefix without modifying argv
     inconclusive, terminal, or no command
       => GitPassthrough(original argv, GitGlobalOrMalformed)
  5. exact verb has JjkNative registry entry
       => JjkNative(spec, original globals, original tail)
  6. exact verb has EnhancedGit registry entry
       => EnhancedGit(spec, original argv, spans)
  7. otherwise
       => GitPassthrough(original argv, UnownedVerb)
```

There is no fallback state creation after step 8.

| Invocation | Route and proof |
|---|---|
| `jjk status` | enhanced: native Git status plus JJK orientation in eligible human mode |
| `jjk status --porcelain=v2 -z` | enhanced transparent mode: native machine bytes only |
| `jjk rebase -i HEAD~3` | exact `git rebase -i HEAD~3` |
| `jjk clone URL dst` | exact `git clone URL dst`, valid outside repositories |
| `jjk future-git-command x` | Git decides built-in, alias, helper, or unknown |
| `jjk rebsae` | Git's normal error; no JJK state |
| `jjk "baseline before parser rewrite"` | Git decides alias/helper/unknown; descriptions require `jjk save -- "baseline before parser rewrite"` |
| `jjk baseline` | Git alias/helper/unknown; state creation requires `jjk save baseline` |
| `jjk save -- -leading-dash` | native literal description |
| `jjk rebase -- --literal` | both delimiters reach Git unchanged |
| `jjk git -- status` | native Git status, bypassing enhancement |
| `jjk git --` | bare Git, including native help/exit |
| `jjk -- status` | native `git -- status` error |

### CR-D06 — Routing-aware help and completion

- `jjk help`, `jjk -h`, `jjk --help` show progressive JJK help.
- `jjk help <owned>` shows JJK contract and command class.
- `jjk help <unowned>` invokes `git help <unowned>` with inherited terminal/pager.
- `<verb> --help` follows the selected route: rebase help is Git; status help is Git status help without enrichment; save help is JJK.
- `jjk version`, sole `-v`, and sole `--version` report JJK; `jjk git -- --version` reports Git.

`jjk completion <bash|zsh|fish|powershell>` uses the production registry/parser. It offers native/enhanced commands, obtains Git built-ins/guides from effective Git, includes configured aliases and discoverable helpers where supported, delegates unowned argument completion to shell Git completion, and uses Git-compatible completion for enhanced commands. It performs no initialization, locking, reconciliation, hook, network call, or mutation. Completion hints never affect runtime routing.

### CR-D07 — Passthrough is a process contract

For `GitPassthrough`, resolve real Git and immediately transfer control without reading repository or JJK state.

| Surface | Required behavior |
|---|---|
| executable | verified real Git, directly, without a shell |
| argv | original order and native representation; only `jjk git --` removes its two routing tokens |
| cwd | exact inherited cwd |
| environment | all inherited keys/values unchanged; no injection/removal/rewrite |
| stdin/stdout/stderr | exact inherited handles; never captured, decoded, trimmed, recolored, prefixed, or reordered |
| TTY/PTY | same handles and dimensions |
| pager/editor/credentials | Git owns `/dev/tty`, pinentry, SSH, askpass, editor, pager |
| color | Git sees original TTY/environment; JJK adds no color flag |
| signals/job control | Git receives foreground signals and terminal job control |
| exit | shell observes Git's native exit code or signal termination |
| side effects | no JJK event, projection, lock, auto-init, snapshot, hook, or reconciliation |

On Unix use `CommandExt::exec`/`execve`, making Git the same process. On Windows run Git with inherited handles in the current console, install no input-consuming layer, wait, and exit with its status; console-control parity is release-gated. Launch failures use shell-compatible 126 (not executable/self-resolution) and 127 (not found).

There is no after-Git reconciliation: exact exec and post-processing are mutually exclusive. The next semantic JJK command reconciles external facts.

### CR-D08 — `status` enhancement with transparent automation mode

1. Run real Git with the entire original Git-compatible argv and inherited process surfaces.
2. Preserve Git output as emitted; never parse/re-render it.
3. If Git exits nonzero, emit nothing else and preserve its result.
4. If stdout is not a TTY, emit nothing else.
5. If any status argument follows the verb—including future unknown options—emit nothing else. This conservative rule protects all machine formats.
6. Only for human bare status (plus optional outer globals), after Git succeeds, on a healthy safe space, append one delimited JJK orientation section: semantic state, attempt, metadata freshness, recovery hint.
7. Without a safe space, the result is native Git status.
8. If enrichment fails after Git succeeds, warn concisely on stderr but retain Git success.

Reconciliation for orientation is idempotent and follows the cross-layer protocol. It may append changed external facts but must not alter HEAD, index, worktree, remotes, or Git config. JJK color appears only in the appended TTY section and respects `NO_COLOR`, `TERM=dumb`, and explicit policy; Git's color choice is untouched. `jjk git -- status` is the native escape.

### CR-D09 — Interactive commands, stdin, and signals belong to Git

`rebase -i`, `add -p`, `commit`, `mergetool`, credentials, signing, editors, pagers, and shell aliases are transparent. JJK does not pre-read stdin, change terminal modes, allocate a replacement PTY, capture output, install a pager, translate control bytes, answer prompts, or detach Git into another process group. This preserves binary pipelines and native `SIGPIPE` behavior.

### CR-D10 — Alias/helper resolution is delegated

Unowned tokens reach Git, which performs native resolution of built-ins, `alias.*`, dashed external helpers, exec-path, and PATH.

- `jjk lg` honors `alias.lg=log`.
- `jjk -c alias.review='!tool ...' review` honors invocation-scoped aliases.
- `jjk lfs ...` reaches `git-lfs`.
- future `git foo` works through `jjk foo` without a JJK release.
- alias expansion to `status` stays a Git alias, not recursive JJK enhancement.
- collisions remain reachable through `jjk git -- <name> ...`.

Interactive-shell functions/aliases named `git` are not executable Git features and are not visible to native `execvp("git", ...)`; parity covers the real Git executable, config aliases, and `git-*` helpers.

### CR-D11 — Clone works outside repositories

Routing precedes discovery. From `/tmp`, `jjk clone URL project` directly becomes `git clone URL project`. JJK requires no parent repository, creates no JJK control root, rewrites no destination, and does not auto-enroll the clone. The user opts in later with `jjk setup project`. The same applies to bare clone, `init-db`, `ls-remote`, credentials, version, help, and other outside-repository operations.

### CR-D12 — Recursion prevention by executable identity

A single Git resolver adapter:

1. resolves `git` using inherited PATH rules without a shell;
2. canonicalizes where possible;
3. compares selected file identity with the running JJK executable, following symlinks;
4. rejects self-resolution with exit 126 and actionable paths;
5. caches verified identity for the process;
6. invokes directly, never through `jjk` or generated wrappers.

No recursion environment marker is injected because transparent environment parity is stronger and identity prevents JJK-controlled installation loops. User-authored aliases/helpers that explicitly recurse have native Git responsibility. Internal Git calls from native handlers use the same verified adapter through an API that cannot enter CLI routing.

## Invariants

| ID | Invariant |
|---|---|
| CR-I01 | Exactly one route is selected before repository/JJK-store access. |
| CR-I02 | Owned matches are exact and case-sensitive; no fuzzy routing. |
| CR-I03 | Every unowned legal, unknown, non-UTF-8, future Git, alias, and helper verb goes to Git. |
| CR-I04 | Passthrough performs zero JJK state reads/writes and acquires no safe-space lock. |
| CR-I05 | Passthrough arguments are original `OsString` values. |
| CR-I06 | Passthrough preserves cwd, environment, handles, TTY, signals, and exit disposition. |
| CR-I07 | `jjk clone` works outside repositories and never auto-enrolls its result. |
| CR-I08 | `jjk rebase` is interactive native Git, including editor, prompts, conflicts, and exits. |
| CR-I09 | `status` is the only v0.1 enhanced Git spelling. `jjk status`, `jjk status --format json`, and its aliases `--json`, `--width`, and `--no-color` are JJK-owned orientation forms; unknown/status-porcelain flags passthrough unchanged to Git. |
| CR-I10 | A name collision never makes Git unreachable; `jjk git --` is the lossless escape. |
| CR-I11 | A typo or unknown text cannot create a state; descriptions require `jjk save -- <text>`. |
| CR-I12 | Transparent mutations reconcile later as external facts, never as JJK transactions. |
| CR-I13 | The router introduces no environment marker or hidden stdio protocol. |
| CR-I14 | Real Git cannot resolve to running JJK file identity. |
| CR-I15 | Completion/help consume the execution registry/classifier. |

## Data/API shapes

```rust
use std::{ffi::OsString, ops::Range, path::PathBuf};

pub struct CommandId(pub &'static str);
pub enum CommandClass { JjkNative, EnhancedGit }
pub enum MutationClass { ReadOnly, JjkTransaction, EnhancedPostGit }
pub enum RepoRequirement { None, GitRepository, JjkSafeSpace, GitThenOptionalSafeSpace }

pub struct CommandSpec {
    pub id: CommandId,
    pub spelling: &'static str,
    pub class: CommandClass,
    pub mutates: MutationClass,
    pub repo_requirement: RepoRequirement,
    pub help_topic: &'static str,
}

pub struct RawInvocation { pub argv: Vec<OsString>, pub cwd: PathBuf }
pub struct ParsedGitPrefix { pub global_span: Range<usize>, pub command_index: usize }
pub enum PrefixScan { Command(ParsedGitPrefix), NoCommand, TerminalGitOption, Inconclusive }

pub enum RoutePlan<'a> {
    JjkNative { spec: &'static CommandSpec, globals: &'a [OsString], args: &'a [OsString] },
    EnhancedGit { spec: &'static CommandSpec, original_argv: &'a [OsString], prefix: ParsedGitPrefix },
    GitPassthrough { argv: &'a [OsString], reason: PassthroughReason },
}

pub enum PassthroughReason {
    ExplicitEscape, UnownedVerb, NonUtf8Verb, FutureOrUnknownGlobal,
    MalformedGitGlobal, TerminalGitGlobal,
}

pub struct GitExecutable { pub path: PathBuf, pub identity: ExecutableIdentity }
pub trait GitProcess {
    fn exec_transparent(&self, argv: &[OsString]) -> Result<Never, LaunchError>;
    fn run_inherited(&self, argv: &[OsString]) -> Result<ChildDisposition, LaunchError>;
}
pub enum ChildDisposition { Exited(i32), Signaled(i32) }

pub fn route(invocation: &RawInvocation, registry: &CommandRegistry)
    -> Result<RoutePlan<'_>, RouteError>;
```

`route` is pure/table-driven. Repository discovery, locks, databases, Git probes, telemetry, logging, and color detection are forbidden dependencies.

## Failure modes

| ID | Failure | Required containment |
|---|---|---|
| CR-F01 | Unknown future Git global | pass entire original invocation to Git |
| CR-F02 | Missing global option value | Git emits native usage/error |
| CR-F03 | Mistyped command | native Git error/suggestion; no state |
| CR-F04 | Invocation-scoped alias/helper | Git resolves it |
| CR-F05 | Alias/helper collides with registry | JJK wins; document `jjk git -- ...` |
| CR-F06 | Git absent | stderr diagnostic, exit 127; no initialization |
| CR-F07 | Git resolves to JJK | stop before exec, exit 126 |
| CR-F08 | JJK metadata corrupt during passthrough | never read it; Git runs |
| CR-F09 | Status enrichment fails | return a typed JJK error for owned orientation forms; `jjk git -- status` remains available |
| CR-F10 | Unknown or Git-machine status option | entire original invocation transparently executes Git |
| CR-F11 | Interactive Git needs terminal | inherited handles; no proxy |
| CR-F12 | Git launches editor/pager/credential/signing | preserve environment/process behavior |
| CR-F13 | Downstream pipe closes | native `SIGPIPE`; no wrapper output |
| CR-F14 | Git mutates before JJK use | next native transaction reconciles idempotently |
| CR-F15 | Non-UTF-8 argv | raw passthrough, no lossy conversion |
| CR-F16 | Git binary replaced before exec | identity recheck/fail where supported |
| CR-F17 | Clone outside repo | no preflight; Git owns result |
| CR-F18 | `jjk git --` without payload | invoke bare Git exactly |

## Acceptance checks

All passthrough checks are differential against native Git in equivalent fresh fixtures, without normalizing whitespace, ANSI, streams, or signal results.

1. **Enhanced status:** `jjk status` emits native status truth plus orientation. `jjk status --format json` and `--json` emit the common JJK envelope. Known JJK presentation flags remain enhanced under TTY or redirection. Unknown options and Git machine forms such as `--porcelain=v2 -z` are byte/status-identical passthrough; `jjk git -- status` always has no JJK section.
2. **Native rebase:** PTY fixture with controlled sequence editor compares editor argv, TTY, conflicts, output, signals, and exit; no JJK metadata changes.
3. **Native clone:** from empty non-repositories compare streams, exit, refs, files, modes, config; neither parent nor clone gains JJK metadata.
4. **All unenhanced commands:** generate installed Git command inventory, exclude only `EnhancedGit`, and assert every remainder classifies passthrough; behavior matrices cover read, write, interactive, remote, plumbing, and outside-repo representatives, including a command unknown when JJK compiled.
5. **Raw fidelity probe:** a `git-jjk-probe` helper records argv byte lengths, cwd, complete environment, TTY flags, stdin bytes, and terminal size. Compare native Git and JJK for empty args, spaces, quotes, leading dashes, `--`, Unicode, and Unix non-UTF-8. No JJK environment key may appear.
6. **Aliases/helpers:** cover normal alias, `!` shell alias with stdin/nonzero exit, invocation-scoped alias, PATH helper, exec-path helper, collision escape, and unknown-command suggestions.
7. **Globals/delimiters:** cover attached/separate `-C` and `-c`, long values, pager flags, git-dir/work-tree, malformed value, unknown global, terminal globals, `jjk -- status`, and command-local `--`; original argv never changes.
8. **Process parity:** PTY/pipe fixtures prove editor/pager, credential/askpass, NUL streams, stdout/stderr ordering, Unix signals/job control/`SIGPIPE`, Windows console-control/exit parity, and color under TTY, pipe, `NO_COLOR`, `TERM=dumb`, and Git config.
9. **No side effects:** before/after passthrough compare JJK journal/projection absence or checksum+mtime; lock/temp absence; and no refs/config/hooks/index locks/commits/files beyond native Git.
10. **Registry guard:** build-time comparison against oldest/newest supported Git inventories fails accidental native collisions. `status` is the only approved v0.1 overlap; `setup` owns JJK initialization while `init` remains Git.

## Explicit non-goals

- Emulating Git commands or pre-enumerating aliases/helpers for routing.
- Auto-enrolling clones or appending JJK events after transparent commands.
- Preserving arbitrary interactive-shell functions/aliases named `git`.
- Recoloring, translating, or capturing Git output.
- Treating unknown verbs as descriptions.
- Guessing ownership of unknown `status` flags. The explicit JJK set is `--format json`, `--json`, `--width`, and `--no-color`; every other tail is Git passthrough.
- Reserving global `--`; only `jjk git -- ...` consumes routing tokens.
- Injecting a recursion environment marker.
- Claiming transparent behavior for the single deliberately enhanced spelling.
- Dynamic plugins claiming top-level names in v0.1; they could steal future Git commands.

## User contract

> If the word is a JJK semantic verb, JJK owns it. If it is an owned `status` form, JJK deliberately enhances Git. Everything else is Git, unchanged. When names collide, `jjk git -- ...` always reaches Git.

This proves `jjk status` is enhanced while `jjk init`, `jjk rebase`, `jjk clone`, all present/future unenhanced Git commands, aliases, helpers, interactive commands, and unknown verbs retain native Git behavior.
