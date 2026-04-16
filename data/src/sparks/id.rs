// SPDX-License-Identifier: AGPL-3.0-or-later

//! Hash-based ID generation for sparks and other entities.

use uuid::Uuid;

/// Generate a spark ID using the workshop name as prefix, e.g. `ryve-a1b2c3d4`.
pub fn generate_spark_id(workshop_id: &str) -> String {
    let hex = Uuid::new_v4().simple().to_string();
    format!("{}-{}", workshop_id, &hex[..8])
}

/// Generate a generic short ID for comments, embers, alloys, etc.
pub fn generate_id(prefix: &str) -> String {
    let hex = Uuid::new_v4().simple().to_string();
    format!("{}-{}", prefix, &hex[..8])
}
