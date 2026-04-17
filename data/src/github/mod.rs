// SPDX-License-Identifier: AGPL-3.0-or-later

//! GitHub Issues sync for Workgraph.

pub mod sync;
pub mod types;

pub use sync::GitHubSync;
pub use types::{CanonicalGitHubEvent, GitHubArtifactRef};
