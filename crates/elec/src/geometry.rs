//! Geometric connectivity: turn drawn wires into nets.
//!
//! A schematic is drawn as lines and symbols, but it is *read* as a graph. This module does
//! the conversion: wire segments join at shared endpoints, and a symbol anchors to a net
//! when its connection point lands on a wire endpoint.
//!
//! The rule is deliberately conservative. Two things connect when their points coincide
//! within a tolerance; nothing is inferred from proximity alone, and a wire that crosses
//! another without sharing an endpoint does not merge them (that is what schematic dots
//! are for, and a crossing without a dot is not a connection).

use std::collections::BTreeMap;

/// A point in drawing units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A drawn wire: one segment between two points.
#[derive(Debug, Clone, PartialEq)]
pub struct WireSeg {
    pub id: u64,
    pub from: Point,
    pub to: Point,
}

impl WireSeg {
    pub fn new(id: u64, from: Point, to: Point) -> Self {
        Self { id, from, to }
    }
}

/// Something that attaches to a net at a point: a component's connection point.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub id: u64,
    /// Human-readable identity for reports, e.g. the device tag.
    pub label: String,
    pub at: Point,
    /// The pin this anchor is, when the drawing says so (`"2"` for `X1TERM02`).
    ///
    /// A device can anchor to a net at more than one point, and the pin is what makes a
    /// wire list read `-K1:2` rather than three anonymous `-K1` rows.
    pub pin: Option<String>,
}

impl Anchor {
    pub fn new(id: u64, label: &str, at: Point) -> Self {
        Self {
            id,
            label: label.to_string(),
            at,
            pin: None,
        }
    }

    /// Name the anchor with the pin it belongs to, e.g. `M326` + `2`.
    pub fn with_pin(mut self, pin: &str) -> Self {
        self.pin = Some(pin.to_string());
        self
    }

    /// The label a report shows: `TAG:pin` when the pin is known, else the tag.
    pub fn display_label(&self) -> String {
        match &self.pin {
            Some(pin) if !pin.is_empty() => format!("{}:{}", self.label, pin),
            _ => self.label.clone(),
        }
    }
}

/// Connectivity evidence the wire geometry alone cannot express.
///
/// Two things on a real schematic are not a shared point:
///
/// * **Terminal pass-through** — two wires end on opposite sides of one terminal block,
///   `0.125` apart. They are the same equipotential, so the two points are *bridged*.
/// * **Junction dots** — a wire ending on the middle of another wire is a connection only
///   where a dot marks it; a bare crossing is not. When `require_junctions` is set, the
///   T-junction merge only fires at a point in `junctions`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetlistHints {
    /// Point pairs that are one node although they are apart (a wire through a terminal).
    pub bridges: Vec<(Point, Point)>,
    /// Junction markers (dots) on the drawing.
    pub junctions: Vec<Point>,
    /// When true, a T-junction needs one of `junctions`; when false, any endpoint landing
    /// on another wire's interior connects (the historical behaviour).
    pub require_junctions: bool,
}

impl NetlistHints {
    /// True when a junction dot sits on `point` (within `JUNCTION_SNAP`).
    fn has_junction(&self, point: Point) -> bool {
        const JUNCTION_SNAP: f64 = 0.05;
        self.junctions.iter().any(|dot| {
            (dot.x - point.x).abs() <= JUNCTION_SNAP && (dot.y - point.y).abs() <= JUNCTION_SNAP
        })
    }
}

/// One electrical net found in the geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoNet {
    /// Position in the net list, 1-based. Nets are ordered by the lowest wire or
    /// anchor id that joined them, then renumbered, because the two id spaces
    /// overlap and a raw id would name two different nets the same thing.
    pub id: u64,
    /// Anchored component ids, ascending.
    pub anchors: Vec<u64>,
    /// Wire ids, ascending.
    pub wires: Vec<u64>,
}

/// Result of the geometric pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeoNetlist {
    pub nets: Vec<GeoNet>,
    /// Anchors that no wire reached.
    pub loose_anchors: Vec<u64>,
    /// Wires in no net (a wire with no anchor and no other wire).
    pub loose_wires: Vec<u64>,
}

/// Grid cell a point falls into. Coordinates are divided by the tolerance and rounded, so
/// two endpoints within half a cell of each other land on the same node.
type Cell = (i64, i64);

fn cell(point: Point, tolerance: f64) -> Cell {
    let tol = if tolerance > 0.0 { tolerance } else { 1e-6 };
    (
        (point.x / tol).round() as i64,
        (point.y / tol).round() as i64,
    )
}

/// Union-find over grid cells.
struct DisjointSet {
    parent: BTreeMap<Cell, Cell>,
}

impl DisjointSet {
    fn new() -> Self {
        Self {
            parent: BTreeMap::new(),
        }
    }

    fn find(&mut self, node: Cell) -> Cell {
        let parent = *self.parent.entry(node).or_insert(node);
        if parent == node {
            return node;
        }
        let root = self.find(parent);
        self.parent.insert(node, root);
        root
    }

    fn union(&mut self, a: Cell, b: Cell) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let (lo, hi) = if (ra.0, ra.1) <= (rb.0, rb.1) { (ra, rb) } else { (rb, ra) };
        self.parent.insert(hi, lo);
    }
}

/// Build nets from drawn wires and component anchors.
///
/// `tolerance` is the distance below which two points are the same node. A typical drawing
/// uses something around the snap grid (0.1 mm at 1:1, or 1 unit at schematic scale).
/// Shortest distance from a point to a line segment, used to recognise a T-junction.
fn point_to_segment_distance(point: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return ((point.x - a.x).powi(2) + (point.y - a.y).powi(2)).sqrt();
    }
    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    let cx = a.x + t * dx;
    let cy = a.y + t * dy;
    ((point.x - cx).powi(2) + (point.y - cy).powi(2)).sqrt()
}

pub fn netlist_from_geometry(wires: &[WireSeg], anchors: &[Anchor], tolerance: f64) -> GeoNetlist {
    netlist_from_geometry_impl(wires, anchors, tolerance, false, None)
}

/// Like [`netlist_from_geometry`], but a run of wires with no component on it is still a net.
///
/// Electrically a connected wire run is an equipotential whether or not a symbol happens to
/// sit on it, and a drawing whose symbols were exploded has nothing else to anchor to. The
/// runs appear in `nets` with an empty `anchors` list; `loose_wires` stays empty.
pub fn netlist_from_geometry_with_wire_runs(
    wires: &[WireSeg],
    anchors: &[Anchor],
    tolerance: f64,
) -> GeoNetlist {
    netlist_from_geometry_impl(wires, anchors, tolerance, true, None)
}

/// The full pass: wire runs, anchors, terminal bridges and dot-verified junctions.
///
/// This is what the electrical commands use. `hints` carries the evidence that is not a
/// shared point (see [`NetlistHints`]); the plain functions above stay for callers and
/// tests that only have bare geometry.
pub fn netlist_with_hints(
    wires: &[WireSeg],
    anchors: &[Anchor],
    tolerance: f64,
    hints: &NetlistHints,
    keep_wire_runs: bool,
) -> GeoNetlist {
    netlist_from_geometry_impl(wires, anchors, tolerance, keep_wire_runs, Some(hints))
}

fn netlist_from_geometry_impl(
    wires: &[WireSeg],
    anchors: &[Anchor],
    tolerance: f64,
    keep_wire_runs: bool,
    hints: Option<&NetlistHints>,
) -> GeoNetlist {
    let mut dsu = DisjointSet::new();
    for wire in wires {
        let a = cell(wire.from, tolerance);
        let b = cell(wire.to, tolerance);
        dsu.union(a, b);
    }
    if let Some(hints) = hints {
        // A wire that passes through a terminal stops 0.125 short of it on each side: the
        // two ends are the same equipotential even though no point is shared.
        for (a, b) in &hints.bridges {
            dsu.union(cell(*a, tolerance), cell(*b, tolerance));
        }
    }
    if keep_wire_runs {
        // A wire that ends on the middle of another wire is a T-junction: the same net.
        // That is what a junction dot means on a schematic, and endpoint-only unioning left
        // real drawings with one net per segment. The gap allowed is deliberately small
        // (0.02 drawing units) so 0.125-unit symbol-pin gaps and wire-hop arcs do not merge.
        const T_JUNCTION: f64 = 0.02;
        let junction_allowed = |point: Point| -> bool {
            match hints {
                Some(hints) if hints.require_junctions => hints.has_junction(point),
                _ => true,
            }
        };
        for wire in wires {
            for other in wires {
                if wire.id == other.id {
                    continue;
                }
                for endpoint in [wire.from, wire.to] {
                    if point_to_segment_distance(endpoint, other.from, other.to) <= T_JUNCTION
                        && junction_allowed(endpoint)
                    {
                        dsu.union(cell(endpoint, tolerance), cell(other.from, tolerance));
                    }
                }
            }
        }
    }

    for anchor in anchors {
        // Registering the anchor's node is what lets a symbol with no wires still be
        // reported as unconnected rather than silently vanishing.
        dsu.find(cell(anchor.at, tolerance));
    }

    // Group wires and anchors by their root cell.
    let mut by_root: BTreeMap<Cell, (Vec<u64>, Vec<u64>)> = BTreeMap::new();
    for wire in wires {
        let root = dsu.find(cell(wire.from, tolerance));
        by_root.entry(root).or_default().0.push(wire.id);
    }
    let mut anchor_root: BTreeMap<u64, Cell> = BTreeMap::new();
    for anchor in anchors {
        let root = dsu.find(cell(anchor.at, tolerance));
        by_root.entry(root).or_default().1.push(anchor.id);
        anchor_root.insert(anchor.id, root);
    }

    let mut nets = Vec::new();
    let mut loose_anchors = Vec::new();
    let mut loose_wires = Vec::new();

    for (_root, (mut wire_ids, mut anchor_ids)) in by_root {
        wire_ids.sort_unstable();
        anchor_ids.sort_unstable();
        if wire_ids.is_empty() && anchor_ids.len() == 1 {
            // A symbol alone is not a net; it is an unconnected terminal.
            loose_anchors.extend(anchor_ids);
            continue;
        }
        if anchor_ids.is_empty() {
            // Wires with no symbol on them still form a net electrically (a cable run
            // between two sheets). Callers that want the run as a net ask for it; the
            // default keeps reporting them as loose drafting for the strict callers.
            if keep_wire_runs {
                let id = wire_ids.first().copied().unwrap_or(0);
                nets.push(GeoNet {
                    id,
                    anchors: Vec::new(),
                    wires: wire_ids,
                });
            } else {
                loose_wires.extend(wire_ids);
            }
            continue;
        }
        let id = wire_ids
            .first()
            .copied()
            .or_else(|| anchor_ids.first().copied())
            .unwrap_or(0);
        nets.push(GeoNet {
            id,
            anchors: anchor_ids,
            wires: wire_ids,
        });
    }

    loose_anchors.sort_unstable();
    loose_wires.sort_unstable();
    // A net's birth id is the first wire or the first anchor that joined it, and
    // those are two different id spaces: a wire-only run and a net made of pins
    // alone can both claim `1`, so the printed net list repeated one label and
    // skipped another, and the CSV rows carried the same duplicate. Order by the
    // birth id, then hand out one sequence, so `ELECWIRELIST`, the wire rows and
    // the automation read all name a net the same way.
    nets.sort_by_key(|net| net.id);
    for (index, net) in nets.iter_mut().enumerate() {
        net.id = (index + 1) as u64;
    }
    GeoNetlist {
        nets,
        loose_anchors,
        loose_wires,
    }
}

/// One row of the wire list report.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoWireRow {
    pub wire: u64,
    pub net: u64,
    pub from: Point,
    pub to: Point,
    /// Device pin at the `from` end, when an anchor pin sits on it (`-K1:1`).
    pub from_pin: Option<String>,
    /// Device pin at the `to` end.
    pub to_pin: Option<String>,
}

/// The wire endpoint a pin anchor must be within to name that end of a conductor.
const PIN_ON_WIRE: f64 = 0.2;

fn pin_label_near(anchors: &[Anchor], point: Point) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    for anchor in anchors {
        let distance = (anchor.at.x - point.x).hypot(anchor.at.y - point.y);
        if distance <= PIN_ON_WIRE
            && best.as_ref().map(|(seen, _)| distance < *seen).unwrap_or(true)
        {
            best = Some((distance, anchor.display_label()));
        }
    }
    best.map(|(_, label)| label)
}

/// Project the wires of a netlist into report rows, ordered by wire id.
///
/// The rows carry no pin names; use [`wire_rows_with_pins`] when the anchors are known.
pub fn wire_rows(wires: &[WireSeg], netlist: &GeoNetlist) -> Vec<GeoWireRow> {
    wire_rows_impl(wires, netlist, &[])
}

/// Wire-list rows that also name the device pin at each end of the conductor.
///
/// A wire list an electrician can pull from says `-K1:2 -> -M1:1`, not two coordinates.
/// The pin is the label of the nearest anchor within [`PIN_ON_WIRE`]; an end with no device
/// on it (a wire to a terminal block, a spare) stays empty.
pub fn wire_rows_with_pins(
    wires: &[WireSeg],
    netlist: &GeoNetlist,
    anchors: &[Anchor],
) -> Vec<GeoWireRow> {
    wire_rows_impl(wires, netlist, anchors)
}

fn wire_rows_impl(wires: &[WireSeg], netlist: &GeoNetlist, anchors: &[Anchor]) -> Vec<GeoWireRow> {
    let mut net_of_wire: BTreeMap<u64, u64> = BTreeMap::new();
    for net in &netlist.nets {
        for wire in &net.wires {
            net_of_wire.insert(*wire, net.id);
        }
    }
    let mut rows: Vec<GeoWireRow> = wires
        .iter()
        .filter_map(|wire| {
            net_of_wire.get(&wire.id).map(|net| GeoWireRow {
                wire: wire.id,
                net: *net,
                from: wire.from,
                to: wire.to,
                from_pin: pin_label_near(anchors, wire.from),
                to_pin: pin_label_near(anchors, wire.to),
            })
        })
        .collect();
    rows.sort_by_key(|row| row.wire);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn two_symbols_joined_by_one_wire_form_a_net() {
        let wires = vec![WireSeg::new(1, p(0.0, 0.0), p(100.0, 0.0))];
        let anchors = vec![Anchor::new(10, "-K1", p(0.0, 0.0)), Anchor::new(11, "-K2", p(100.0, 0.0))];
        let netlist = netlist_from_geometry(&wires, &anchors, 0.01);
        assert_eq!(netlist.nets.len(), 1);
        assert_eq!(netlist.nets[0].anchors, vec![10, 11]);
        assert_eq!(netlist.nets[0].wires, vec![1]);
        assert_eq!(netlist.nets[0].id, 1);
    }

    #[test]
    fn chained_wires_merge_into_one_net() {
        let wires = vec![
            WireSeg::new(1, p(0.0, 0.0), p(50.0, 0.0)),
            WireSeg::new(2, p(50.0, 0.0), p(100.0, 0.0)),
            WireSeg::new(3, p(100.0, 0.0), p(150.0, 0.0)),
        ];
        let anchors = vec![Anchor::new(10, "-K1", p(0.0, 0.0)), Anchor::new(11, "-K2", p(150.0, 0.0))];
        let netlist = netlist_from_geometry(&wires, &anchors, 0.01);
        assert_eq!(netlist.nets.len(), 1);
        assert_eq!(netlist.nets[0].wires, vec![1, 2, 3]);
        assert_eq!(netlist.nets[0].anchors, vec![10, 11]);
    }

    #[test]
    fn crossing_wires_without_a_shared_point_stay_separate() {
        // A horizontal and a vertical wire crossing at (50,0) but neither ending there.
        let wires = vec![
            WireSeg::new(1, p(0.0, 0.0), p(100.0, 0.0)),
            WireSeg::new(2, p(50.0, -50.0), p(50.0, 50.0)),
        ];
        let anchors = vec![
            Anchor::new(10, "-K1", p(0.0, 0.0)),
            Anchor::new(11, "-K2", p(50.0, 50.0)),
        ];
        let netlist = netlist_from_geometry(&wires, &anchors, 0.01);
        assert_eq!(netlist.nets.len(), 2);
    }

    #[test]
    fn a_symbol_on_no_wire_is_reported_loose() {
        let wires = vec![WireSeg::new(1, p(0.0, 0.0), p(100.0, 0.0))];
        let anchors = vec![Anchor::new(10, "-K1", p(0.0, 0.0)), Anchor::new(11, "-K9", p(500.0, 500.0))];
        let netlist = netlist_from_geometry(&wires, &anchors, 0.01);
        assert_eq!(netlist.loose_anchors, vec![11]);
        assert_eq!(netlist.nets.len(), 1);
    }

    #[test]
    fn wires_with_no_symbol_are_reported_loose() {
        let wires = vec![WireSeg::new(7, p(0.0, 0.0), p(10.0, 0.0))];
        let netlist = netlist_from_geometry(&wires, &[], 0.01);
        assert!(netlist.nets.is_empty());
        assert_eq!(netlist.loose_wires, vec![7]);
    }

    #[test]
    fn tolerance_joins_near_miss_endpoints() {
        let wires = vec![
            WireSeg::new(1, p(0.0, 0.0), p(50.0, 0.0)),
            WireSeg::new(2, p(50.02, 0.0), p(100.0, 0.0)),
        ];
        let anchors = vec![Anchor::new(10, "-K1", p(0.0, 0.0)), Anchor::new(11, "-K2", p(100.0, 0.0))];
        let joined = netlist_from_geometry(&wires, &anchors, 0.5);
        assert_eq!(joined.nets.len(), 1);
        let apart = netlist_from_geometry(&wires, &anchors, 0.001);
        assert!(apart.nets.is_empty() || apart.nets.len() == 2);
    }

    #[test]
    fn net_identity_is_stable_under_input_order() {
        let wires = vec![
            WireSeg::new(2, p(50.0, 0.0), p(100.0, 0.0)),
            WireSeg::new(1, p(0.0, 0.0), p(50.0, 0.0)),
        ];
        let anchors = vec![Anchor::new(10, "-K1", p(0.0, 0.0)), Anchor::new(11, "-K2", p(100.0, 0.0))];
        let netlist = netlist_from_geometry(&wires, &anchors, 0.01);
        assert_eq!(netlist.nets.len(), 1);
        assert_eq!(netlist.nets[0].wires, vec![1, 2]);
        assert_eq!(netlist.nets[0].id, 1);
    }

    #[test]
    fn coincident_symbols_form_a_net_with_no_wires() {
        // Two symbols dropped on the same point are electrically joined by contact, but
        // there is no wire to hang a wire number on. Consumers must cope with an empty
        // wire list rather than indexing it.
        let anchors = vec![
            Anchor::new(10, "WD_M", p(50.0, 50.0)),
            Anchor::new(11, "WD_M", p(50.0, 50.0)),
        ];
        let netlist = netlist_from_geometry(&[], &anchors, 0.01);
        assert_eq!(netlist.nets.len(), 1);
        assert!(netlist.nets[0].wires.is_empty());
        assert_eq!(netlist.nets[0].anchors, vec![10, 11]);
    }

    #[test]
    fn wire_rows_carry_the_net() {
        let wires = vec![
            WireSeg::new(1, p(0.0, 0.0), p(50.0, 0.0)),
            WireSeg::new(2, p(50.0, 0.0), p(100.0, 0.0)),
            WireSeg::new(9, p(0.0, 90.0), p(10.0, 90.0)),
        ];
        let anchors = vec![Anchor::new(10, "-K1", p(0.0, 0.0)), Anchor::new(11, "-K2", p(100.0, 0.0))];
        let netlist = netlist_from_geometry(&wires, &anchors, 0.01);
        let rows = wire_rows(&wires, &netlist);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].wire, 1);
        assert_eq!(rows[1].wire, 2);
        assert_eq!(rows[0].net, 1);
        assert_eq!(rows[1].net, 1);
    }

    // ---- pins and hints ------------------------------------------------------------

    #[test]
    fn an_anchor_can_name_its_pin() {
        let anchor = Anchor::new(10, "M326", p(5.0, 5.0)).with_pin("2");
        assert_eq!(anchor.pin.as_deref(), Some("2"));
        assert_eq!(anchor.display_label(), "M326:2");
        let bare = Anchor::new(11, "-K1", p(0.0, 0.0));
        assert_eq!(bare.display_label(), "-K1");
    }

    #[test]
    fn a_wire_run_and_a_pin_only_net_do_not_share_a_label() {
        // The run is named by its first wire, the pin-only net by its first
        // anchor. Those ids come from different counters and both are 1 here, so
        // the net list used to print `net 1` twice and skip `net 2`.
        let wires = vec![WireSeg::new(1, p(0.0, 10.0), p(50.0, 10.0))];
        let anchors = vec![
            Anchor::new(1, "-K1", p(0.0, 0.0)).with_pin("1"),
            Anchor::new(2, "-K2", p(0.0, 0.0)).with_pin("2"),
        ];
        let netlist = netlist_from_geometry_with_wire_runs(&wires, &anchors, 0.01);
        assert_eq!(netlist.nets.len(), 2);
        let ids: Vec<u64> = netlist.nets.iter().map(|net| net.id).collect();
        assert_eq!(ids, vec![1, 2], "one net list, one sequence of labels");
    }

    #[test]
    fn two_pins_on_one_device_anchor_separately() {
        // A contact has two pins; a wire reaches only one of them.
        let wires = vec![WireSeg::new(1, p(10.0, 0.0), p(50.0, 0.0))];
        let anchors = vec![
            Anchor::new(10, "K1", p(10.0, 0.0)).with_pin("1"),
            Anchor::new(11, "K1", p(0.0, 0.0)).with_pin("2"),
        ];
        let netlist = netlist_from_geometry_with_wire_runs(&wires, &anchors, 0.01);
        assert_eq!(netlist.nets.len(), 1);
        assert_eq!(netlist.nets[0].anchors, vec![10]);
        assert_eq!(netlist.loose_anchors, vec![11]);
    }

    #[test]
    fn a_terminal_bridge_joins_the_two_wire_ends() {
        // A wire stops 0.0625 short of the terminal on each side: no shared point.
        let wires = vec![
            WireSeg::new(1, p(0.0, 0.0), p(9.9375, 0.0)),
            WireSeg::new(2, p(10.0625, 0.0), p(20.0, 0.0)),
        ];
        let anchors = vec![
            Anchor::new(10, "-K1", p(0.0, 0.0)).with_pin("1"),
            Anchor::new(11, "-K2", p(20.0, 0.0)).with_pin("2"),
        ];
        let apart = netlist_from_geometry_with_wire_runs(&wires, &anchors, 0.01);
        assert_eq!(apart.nets.len(), 2, "without the bridge the two wires stay apart");

        let hints = NetlistHints {
            bridges: vec![(p(9.9375, 0.0), p(10.0625, 0.0))],
            ..Default::default()
        };
        let joined = netlist_with_hints(&wires, &anchors, 0.01, &hints, true);
        assert_eq!(joined.nets.len(), 1);
        assert_eq!(joined.nets[0].wires, vec![1, 2]);
        assert_eq!(joined.nets[0].anchors, vec![10, 11]);
    }

    #[test]
    fn a_t_junction_needs_a_dot_when_the_drawing_uses_dots() {
        // A branch wire ends on the middle of a straight run: the classic T.
        let wires = vec![
            WireSeg::new(1, p(0.0, 0.0), p(100.0, 0.0)),
            WireSeg::new(2, p(50.0, 0.0), p(50.0, 40.0)),
        ];
        let anchors = vec![
            Anchor::new(10, "-K1", p(0.0, 0.0)).with_pin("1"),
            Anchor::new(11, "-K2", p(50.0, 40.0)).with_pin("2"),
        ];
        // Historical behaviour: any T merges.
        let merged = netlist_from_geometry_with_wire_runs(&wires, &anchors, 0.01);
        assert_eq!(merged.nets.len(), 1);

        // With dot discipline on and no dot, the branch is a crossing: two nets.
        let strict = NetlistHints {
            require_junctions: true,
            ..Default::default()
        };
        let split = netlist_with_hints(&wires, &anchors, 0.01, &strict, true);
        assert_eq!(split.nets.len(), 2);

        // A dot at the T makes it one net again.
        let dotted = NetlistHints {
            junctions: vec![p(50.0, 0.0)],
            require_junctions: true,
            ..Default::default()
        };
        let joined = netlist_with_hints(&wires, &anchors, 0.01, &dotted, true);
        assert_eq!(joined.nets.len(), 1);
    }

    #[test]
    fn a_hop_gap_stays_apart_at_wire_tolerance() {
        // A wire hop leaves 0.125 units of air at the crossing; it must not be bridged.
        let wires = vec![
            WireSeg::new(1, p(0.0, 0.0), p(49.9375, 0.0)),
            WireSeg::new(2, p(50.0625, 0.0), p(100.0, 0.0)),
            WireSeg::new(3, p(50.0, -20.0), p(50.0, 20.0)),
        ];
        let netlist = netlist_from_geometry_with_wire_runs(&wires, &[], 0.05);
        // The two halves of the horizontal run stay separate; the vertical is its own net.
        assert!(netlist.nets.len() >= 2);
    }

    #[test]
    fn wire_rows_name_the_pin_at_each_end() {
        let wires = vec![WireSeg::new(1, p(0.0, 0.0), p(100.0, 0.0))];
        let anchors = vec![
            Anchor::new(10, "-K1", p(0.0, 0.0)).with_pin("1"),
            Anchor::new(11, "-M1", p(100.0, 0.0)).with_pin("2"),
        ];
        let netlist = netlist_from_geometry_with_wire_runs(&wires, &anchors, 0.01);
        let rows = wire_rows_with_pins(&wires, &netlist, &anchors);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].from_pin.as_deref(), Some("-K1:1"));
        assert_eq!(rows[0].to_pin.as_deref(), Some("-M1:2"));

        // Without the anchors the pin columns stay empty (the plain rows).
        let bare = wire_rows(&wires, &netlist);
        assert_eq!(bare[0].from_pin, None);
        assert_eq!(bare[0].to_pin, None);
    }
}
