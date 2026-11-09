//! Reusing one frame's geometry across the repaints that did not change it.
//!
//! The graph's canvas is repainted for reasons that have nothing to do with the graph.
//! gpui rebuilds a window's whole scene on any `notify`, and in the graph's own tab the
//! counts row's `fps` readout notifies once a second; moving the pointer over the canvas
//! notifies on every hover change; the tab bar, the sidebar and the review strip all
//! notify into the same frame. Every one of those repaints used to tessellate all
//! ~18,500 links again, which at whole-vault zoom is 2.57 ms of a 3.38 ms frame — 76% of
//! the graph's paint cost spent reproducing lines that had not moved.
//!
//! So the tessellated link paths are kept, under a [`Key`] describing the picture they
//! were made for, and a repaint that matches the key hands gpui clones of them instead of
//! recomputing them. What that does *not* buy is skipping the scene: gpui's scene is
//! rebuilt from nothing every frame, and [`gpui::Window::paint_path`] copies a path's
//! vertices twice on the way in (`scale` for the device factor, then again into the
//! scene's own array), so the vertex traffic is paid either way. The cache removes the
//! culling pass, the per-link arithmetic, and the batch allocations — not the drawing.
//!
//! Links only, deliberately. They are three quarters of the cost and the only phase a
//! hover change leaves untouched; the nodes are 0.66 ms and the hovered node is one of
//! them, so caching them would throw the cache away on exactly the repaints that are
//! already cheap.

use std::rc::Rc;

use gpui::{Bounds, Path, Pixels};
use shale_core::graph::geom::Camera;
use shale_core::graph::proj::Cam3;

use super::frame::GraphFrame;

/// The picture a cached batch was made for. Every part of it changes the geometry: the
/// frame says where the nodes are, the camera says where the picture is, and the canvas
/// says what is culled.
#[derive(Clone)]
pub(crate) struct Key {
    /// The frame the geometry came from, compared by **identity**.
    ///
    /// A frame is built once by the simulation and afterwards only ever replaced, never
    /// edited, so the same `Rc` is the same positions. Comparing them by value would walk
    /// five thousand nodes to answer a question the pointer already answers, and the view
    /// hands over a fresh `Rc` with every frame precisely so that identity means
    /// something.
    frame: Rc<GraphFrame>,
    /// The camera that placed it — in whichever space the tab is showing. A pan, a
    /// zoom, or a **rotation** moves every vertex, which is why the 3D camera is part
    /// of the key rather than a detail of the projection (roadmap P3.6): a cached
    /// batch belongs to one orientation and to nothing else.
    geometry: Geometry,
    /// The canvas it was culled to, which is also the content mask it was clipped by.
    bounds: Bounds<Pixels>,
}

impl Key {
    /// The key for the picture this paint is about to draw.
    pub(crate) fn new(frame: &Rc<GraphFrame>, geometry: Geometry, bounds: Bounds<Pixels>) -> Self {
        Self {
            frame: frame.clone(),
            geometry,
            bounds,
        }
    }

    /// Whether two keys describe the same picture, and so the same geometry.
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.frame, &other.frame)
            && same_geometry(&self.geometry, &other.geometry)
            && self.bounds == other.bounds
    }
}

/// Which space a cached batch was tessellated in.
#[derive(Clone, Copy)]
pub(crate) enum Geometry {
    /// The flat graph, seen through the 2D camera.
    Flat(Camera),
    /// The projected graph, seen through the 3D camera.
    Deep(Cam3),
}

/// Cameras are compared field by field: `Camera` already derives what it needs, and
/// core's `Cam3` is a plain `Copy` struct that does not, and stays that way.
fn same_geometry(a: &Geometry, b: &Geometry) -> bool {
    match (a, b) {
        (Geometry::Flat(a), Geometry::Flat(b)) => a == b,
        (Geometry::Deep(a), Geometry::Deep(b)) => {
            a.yaw == b.yaw
                && a.pitch == b.pitch
                && a.pan == b.pan
                && a.zoom == b.zoom
                && a.dist == b.dist
        }
        // A mode switch is a new picture even when the pan and zoom match.
        _ => false,
    }
}

/// One colour's worth of tessellated links, with the key that produced them.
struct Batch {
    /// What these paths were drawn for.
    key: Key,
    /// The paths, in the order they are painted, each already inside gpui's vertex
    /// ceiling. Held rather than painted because a path is consumed by
    /// [`gpui::Window::paint_path`], so replaying one means cloning it either way.
    paths: Vec<Path<Pixels>>,
}

/// The geometry kept between paints of the graph's canvas.
///
/// It lives in a cell inside [`super::GraphView`] for the same reason the canvas bounds
/// and the frame counter do: the paint phase cannot reach into the entity it is painting.
/// One cache per view, so two windows over the same vault each keep their own camera's
/// geometry rather than invalidating each other's.
pub(crate) struct Cache {
    /// The links, when a batch is held for the current key.
    links: Option<Batch>,
    /// The key the last paint used, whether or not that paint filled the cache.
    ///
    /// It is what makes [`Self::worth_keeping`] able to tell a settling layout from a
    /// settled one: a key seen twice is a picture that has stopped moving.
    last: Option<Key>,
}

impl Cache {
    /// An empty cache: the first frame of a session.
    pub(crate) fn new() -> Self {
        Self {
            links: None,
            last: None,
        }
    }

    /// The links held for `key`, or `None` when they have to be tessellated again.
    pub(crate) fn links(&self, key: &Key) -> Option<&[Path<Pixels>]> {
        self.links
            .as_ref()
            .filter(|batch| batch.key.same(key))
            .map(|batch| batch.paths.as_slice())
    }

    /// Stores a frame's links under `key`, replacing whatever batch was held.
    pub(crate) fn store_links(&mut self, key: Key, paths: Vec<Path<Pixels>>) {
        self.links = Some(Batch { key, paths });
    }

    /// Whether the links painted for `key` are worth keeping.
    ///
    /// **The first paint for a key is not.** While the layout settles the simulation
    /// ships a new frame every tick, so every key is used exactly once, and keeping its
    /// links would clone a few megabytes of vertices into a batch the next tick throws
    /// away — a cost paid on the frames that are already the tightest budget in the tab.
    /// A key seen a second time is the picture having stopped moving, which is exactly
    /// when the next repaint is likely to come from somewhere else in the window and want
    /// the same geometry back unchanged.
    pub(crate) fn worth_keeping(&self, key: &Key) -> bool {
        self.last.as_ref().is_some_and(|last| last.same(key))
    }

    /// Records that this paint used `key`, so the next one can tell whether the picture
    /// moved.
    pub(crate) fn used(&mut self, key: Key) {
        self.last = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    /// The key for a view of `frame` at `zoom`, on a 100×100 canvas at the origin.
    fn key(frame: &Rc<GraphFrame>, zoom: f32) -> Key {
        Key::new(
            frame,
            Geometry::Flat(Camera {
                pan: [0.0, 0.0],
                zoom,
            }),
            Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(100.0), px(100.0)),
            },
        )
    }

    /// A batch tessellated for the flat view is not a batch the projected view can
    /// replay, even at the same pan and zoom: the third axis moved every vertex.
    #[test]
    fn a_flat_batch_never_answers_a_projected_picture() {
        let frame = Rc::new(GraphFrame::empty());
        let bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(100.0), px(100.0)),
        };
        let mut cache = Cache::new();
        cache.store_links(
            Key::new(
                &frame,
                Geometry::Flat(Camera {
                    pan: [50.0, 50.0],
                    zoom: 1.0,
                }),
                bounds,
            ),
            vec![gpui::Path::new(point(px(0.0), px(0.0)))],
        );
        let deep = Key::new(&frame, Geometry::Deep(Cam3::new()), bounds);
        assert!(
            cache.links(&deep).is_none(),
            "the other space is a different picture"
        );
    }

    /// A batch held for one picture is answered to that picture and to nothing else: the
    /// camera and the canvas decide where every vertex goes, and the frame decides where
    /// every node is.
    #[test]
    fn a_cached_batch_answers_only_its_own_picture() {
        let frame = Rc::new(GraphFrame::empty());
        let mut cache = Cache::new();
        assert!(
            cache.links(&key(&frame, 1.0)).is_none(),
            "an empty cache holds nothing"
        );

        cache.store_links(
            key(&frame, 1.0),
            vec![gpui::Path::new(point(px(0.0), px(0.0)))],
        );
        assert_eq!(
            cache.links(&key(&frame, 1.0)).map(|paths| paths.len()),
            Some(1),
            "the same picture hits"
        );
        assert!(
            cache.links(&key(&frame, 2.0)).is_none(),
            "a zoom is a different picture"
        );

        let other = Rc::new(GraphFrame::empty());
        assert!(
            cache.links(&key(&other, 1.0)).is_none(),
            "a different frame is a different picture even when its nodes are identical"
        );
    }

    /// The policy that keeps a settling layout from paying for a batch it throws away on
    /// the next tick.
    #[test]
    fn a_settling_layout_never_fills_the_cache() {
        let frame = Rc::new(GraphFrame::empty());
        let mut cache = Cache::new();
        let settled = key(&frame, 1.0);
        assert!(
            !cache.worth_keeping(&settled),
            "nothing has been painted yet, so nothing has been seen twice"
        );

        cache.used(settled.clone());
        assert!(
            !cache.worth_keeping(&key(&Rc::new(GraphFrame::empty()), 1.0)),
            "the next frame of a settling layout is a new key, and its links are not kept"
        );
        assert!(
            cache.worth_keeping(&settled),
            "the same picture twice is the layout having stopped moving"
        );
    }
}
