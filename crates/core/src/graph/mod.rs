//! The vault's link graph: what the graph view draws and what the force
//! simulation moves.
//!
//! The graph is built from the in-memory vault index (the same
//! [`crate::markdown::extract_wikilinks`] + [`crate::vault::Vault::resolve_link`]
//! path the backlinks panel uses), so it is always consistent with what the
//! editor already knows. When the `.shale` SQLite DB lands (roadmap P1), edge
//! extraction moves to the DB's incremental index and this module keeps only
//! the structure and the queries — see graph-view-plan.md.

pub mod filter;
pub mod geom;
pub mod hues;
pub mod index;
pub mod proj;
pub mod quadtree;
pub mod sim;

pub use filter::{apply as apply_filter, GraphFilter};
pub use geom::{local_mask, mst_edges, Camera, Grid, MAX_ZOOM, MIN_ZOOM, ZOOM_STEP};
pub use hues::TagHues;
pub use index::{extract_tags, GraphIndex, NodeKind, NodeMeta};
pub use proj::{Cam3, Projected};
pub use sim::{node_radius, Sim, SimParams, NAIVE_LIMIT};
