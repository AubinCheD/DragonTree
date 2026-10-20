//! Moteur git de DragonTree.
//!
//! Toutes les opérations passent par l'exécutable `git` de l'utilisateur,
//! appelé avec des tableaux d'arguments — jamais via un shell — et des sorties `-z`.
//! Seule exception : [`shell`], qui exécute les lignes de commande écrites par l'utilisateur.

pub mod apply;
pub mod bisect;
pub mod blame;
pub mod branches;
pub mod collapse;
pub mod config;
pub mod conflict;
pub mod diff;
pub mod discovery;
pub mod error;
pub mod export;
pub mod external;
pub mod gitflow;
pub mod graph;
pub mod identity;
pub mod ignore;
pub mod lfs;
pub mod log;
pub mod net;
pub mod oid;
pub mod ops;
pub mod patch;
pub mod progress;
pub mod rebase;
pub mod refs;
pub mod repo;
pub mod runner;
pub mod shell;
pub mod ssh;
pub mod stash;
pub mod stats;
pub mod status;
pub mod submodule;
pub mod worktree;

pub use apply::{ApplyTarget, PatchFile, is_mail_patch};
pub use bisect::{BisectMark, BisectState};
pub use branches::{BranchOutcome, StaleBranch, StaleReason};
pub use collapse::{CollapsedLog, collapse_merges};
pub use diff::{ChangeKind, DiffLine, DiffOptions, FileChange, FileDiff, Hunk, LineKind};
pub use error::GitError;
pub use external::{ToolCommand, ToolFiles, ToolVar};
pub use gitflow::{FinishOutcome, FlowKind, FlowStep, GitFlowConfig};
pub use graph::{Edge, GraphBuilder, GraphLayout, GraphRow};
pub use ignore::{IgnorePreview, IgnoreTarget};
pub use lfs::{LfsLock, lfs_available, parse_lfs_patterns, parse_locks};
pub use log::{Commit, LogOptions, LogOrder, LogParser, LogScope, LostCommits, RefSelection};
pub use net::{NetOptions, clone_repo, init_repo, trust_directory};
pub use oid::Oid;
pub use ops::{MergeMode, ResetMode};
pub use patch::{HunkSelection, PatchMode, build_patch};
pub use progress::{Progress, parse_progress};
pub use refs::{HeadState, LocalBranch, RefKind, RefLabel, RefsSnapshot, RemoteBranch, TagRef};
pub use repo::{CommitInfo, OpState, Remote, Repo, Submodule};
pub use runner::{CancelToken, Git, GitCommand, Output};
pub use stash::{StashEntry, fold_stashes};
pub use stats::{Contributor, MonthCount, ObjectStats, RepoStats};
pub use status::{ConflictKind, FileState, StatusEntry, StatusSnapshot};
pub use worktree::{Worktree, WorktreeStart};
