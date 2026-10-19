//! Moteur git de DragonTree.
//!
//! Toutes les opérations passent par l'exécutable `git` de l'utilisateur,
//! appelé avec des tableaux d'arguments — jamais via un shell — et des sorties `-z`.

pub mod blame;
pub mod conflict;
pub mod diff;
pub mod discovery;
pub mod error;
pub mod graph;
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
pub mod stash;
pub mod status;

pub use diff::{ChangeKind, DiffLine, DiffOptions, FileChange, FileDiff, Hunk, LineKind};
pub use error::GitError;
pub use graph::{Edge, GraphBuilder, GraphLayout, GraphRow};
pub use log::{Commit, LogOptions, LogOrder, LogParser, LogScope};
pub use net::{NetOptions, clone_repo, init_repo, trust_directory};
pub use oid::Oid;
pub use ops::{MergeMode, ResetMode};
pub use patch::{HunkSelection, PatchMode, build_patch};
pub use progress::{Progress, parse_progress};
pub use refs::{HeadState, LocalBranch, RefKind, RefLabel, RefsSnapshot, RemoteBranch, TagRef};
pub use repo::{CommitInfo, OpState, Remote, Repo, Submodule};
pub use runner::{CancelToken, Git, GitCommand, Output};
pub use stash::StashEntry;
pub use status::{ConflictKind, FileState, StatusEntry, StatusSnapshot};
