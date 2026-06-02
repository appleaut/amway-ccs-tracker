//! Network / downline chart: a radial node graph. "Me" sits at the centre;
//! direct downline ABOs radiate outward, each deeper level on a wider ring,
//! with straight lines connecting sponsor → downline.
//!
//! Nodes are draggable (offsets are remembered in `AppState.node_offsets`); the
//! "Auto-arrange" button clears those offsets to snap back to the auto layout.

use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;

use crate::app::AppState;
use crate::models::contact::Contact;
use crate::ui::{ACCENT, ACCENT_STRONG};
use crate::utils::scoring;

/// Sentinel key for the central "me" node in the offsets map.
const ME_KEY: i64 = i64::MIN;

/// Zoom bounds for the chart.
const MIN_ZOOM: f32 = 0.4;
const MAX_ZOOM: f32 = 3.0;

/// Highlight colour (amber) for rubber-band-selected nodes and the selection
/// rectangle; the box fill is a translucent tint of it.
const SELECT_RING: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x8F, 0x00);

/// One drawable node. `contact` is `None` for the central "me" node.
struct Node {
    contact: Option<usize>, // index into the `abos` slice
    depth: usize,
    pos: egui::Pos2,
    children: Vec<usize>, // indices into the `nodes` vec
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("เครือข่าย / Downline Chart");
    ui.label(
        egui::RichText::new("ฉันอยู่ตรงกลาง • เส้นเชื่อม = อัพไลน์ → ดาวน์ไลน์")
            .weak()
            .small(),
    );
    ui.add_space(6.0);

    let r = app.db.list_abos();
    let abos = app.handle(r, Vec::new());
    if abos.is_empty() {
        ui.weak("ยังไม่มี ABO ในเครือข่าย — เพิ่มนักธุรกิจที่มีอัพไลน์เพื่อสร้างผังองค์กร");
        return;
    }

    ui.horizontal(|ui| {
        if ui
            .button("จัดผังอัตโนมัติ (Auto-arrange)")
            .on_hover_text("จัดผังใหม่ + รีเซ็ตการลาก node และซูมกลับค่าเริ่มต้น")
            .clicked()
        {
            app.node_offsets.clear();
            app.selected_nodes.clear();
            app.chart_zoom = 1.0;
            app.chart_pan = egui::Vec2::ZERO;
        }
        if ui
            .add(egui::Button::new("📊 ประเมินระดับของฉัน").fill(ACCENT))
            .on_hover_text("ประเมินระดับของฉัน (ME) จากดาวน์ไลน์ตรง + PPV")
            .clicked()
        {
            app.me_advisor = true;
        }
        if ui
            .add(egui::Button::new("💾 บันทึกรูป").fill(ACCENT))
            .on_hover_text("บันทึกภาพผังเครือข่าย (เฉพาะส่วนที่เห็น) เป็นไฟล์ PNG")
            .clicked()
        {
            app.export_chart_pending = true;
        }
        ui.separator();
        ui.label("ซูม:");
        if ui.button(" - ").on_hover_text("ย่อ").clicked() {
            app.chart_zoom = (app.chart_zoom / 1.2).clamp(MIN_ZOOM, MAX_ZOOM);
        }
        ui.label(format!("{:.0}%", app.chart_zoom * 100.0));
        if ui.button(" + ").on_hover_text("ขยาย").clicked() {
            app.chart_zoom = (app.chart_zoom * 1.2).clamp(MIN_ZOOM, MAX_ZOOM);
        }
    });
    ui.label(
        egui::RichText::new(
            "ลาก node เพื่อย้าย • ลากพื้นที่ว่างเพื่อเลือกหลาย node (Shift = เลือกเพิ่ม) แล้วลากย้ายพร้อมกัน • Ctrl + ลาก = เลื่อนมุมมอง (pan) • คลิกพื้นที่ว่างเพื่อยกเลิกการเลือก",
        )
        .weak()
        .small(),
    );
    ui.add_space(4.0);

    // My own qualified rank (from direct downline legs + my PPV), shown inside
    // the central node.
    let (mc1, mcl, mcl15) = app.db.me_leg_counts().unwrap_or((0, 0, 0));
    let me_ppv = app.db.get_me_ppv().unwrap_or(0);
    let me_inside = scoring::qualified_rank(me_ppv, mc1, mcl, mcl15).as_str().to_string();

    // Build sponsor → children adjacency. A node is a "root" (direct downline of
    // me) when it has no sponsor, or its sponsor is not an ABO in the set.
    let ids: HashSet<i64> = abos.iter().map(|c| c.id).collect();
    let mut children_map: HashMap<i64, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, c) in abos.iter().enumerate() {
        match c.sponsor_id {
            Some(sid) if ids.contains(&sid) => children_map.entry(sid).or_default().push(i),
            _ => roots.push(i),
        }
    }

    // Node tree with a virtual "me" root at index 0.
    let mut nodes: Vec<Node> = vec![Node {
        contact: None,
        depth: 0,
        pos: egui::Pos2::ZERO,
        children: Vec::new(),
    }];
    let mut visited: HashSet<i64> = HashSet::new();
    let mut root_nodes = Vec::new();
    for &root in &roots {
        if let Some(n) = build_node(&mut nodes, &abos, &children_map, root, 1, &mut visited) {
            root_nodes.push(n);
        }
    }
    nodes[0].children = root_nodes;

    let mut leaves = vec![0usize; nodes.len()];
    compute_leaves(&nodes, 0, &mut leaves);

    // Angular layout: give every subtree a sector proportional to its leaf
    // count. Angles don't depend on scale, so we fix them once up front.
    let mut angles = vec![0.0f32; nodes.len()];
    assign_angles(&nodes, 0, 0.0, TAU, &leaves, &mut angles);

    let base_node_r = 30.0_f32;
    // Minimum centre-to-centre distance two nodes may sit at before their circles
    // touch (node diameter + a small gap).
    let clearance = 2.0 * base_node_r + 40.0;

    // Per-depth ring radii. Each ring sits just far enough out to (a) clear the
    // ring inside it and (b) separate the closest two nodes that share it. Sizing
    // rings independently keeps sparse / deep branches tight, instead of letting
    // one crowded ring inflate the whole chart — which a single global scale did,
    // spreading every node apart just to satisfy the worst-crammed pair.
    let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
    let mut radii = vec![0.0f32; max_depth + 1];
    for d in 1..=max_depth {
        let sep = min_angle_sep_at_depth(&nodes, &angles, d);
        let r_ang = if sep.is_finite() {
            // chord = 2 r sin(Δ/2) ≥ clearance  ⇒  r ≥ clearance / (2 sin(Δ/2))
            clearance / (2.0 * (sep * 0.5).sin()).max(1e-3)
        } else {
            0.0 // a lone node on this ring needs no angular spreading
        };
        // The 4000px ceiling is just a safety net for pathological trees; real
        // networks stay far below it.
        radii[d] = (radii[d - 1] + clearance).max(r_ang).min(4000.0);
    }

    // Base (zoom = 1) position of every node from its angle and ring radius.
    let base_pos: Vec<egui::Vec2> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| egui::Vec2::angled(angles[i]) * radii[n.depth])
        .collect();

    // Apply the user's zoom uniformly to node size and ring spacing; everything
    // downstream (canvas size, node positions, fonts) derives from these.
    let zoom = app.chart_zoom;
    let node_r = base_node_r * zoom;

    // Bounding box of the auto-layout, whose centre we line up with the viewport
    // centre so the chart starts centred (the user then pans/zooms from there).
    let mut min = egui::vec2(f32::MAX, f32::MAX);
    let mut max = egui::vec2(f32::MIN, f32::MIN);
    for p in &base_pos {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    let layout_center = (min + max) * 0.5;

    let offsets = &mut app.node_offsets;
    let selected = &mut app.selected_nodes;
    let select_start = &mut app.chart_select_start;
    let pan = &mut app.chart_pan;

    // Infinite-canvas navigation: the chart fills the available area and is moved
    // by panning (Ctrl + drag, or the mouse wheel) and zooming — so the view can
    // travel freely in every direction, not only where content overflows a scroll
    // box (which let it pan vertically but not horizontally). `*pan` is the
    // accumulated view offset, added to every node's position.
    //
    // The whole canvas senses click+drag: a drag starting on empty space draws a
    // rubber-band selection box, and a plain click clears the selection. Per-node
    // drag senses are added on top afterwards, so a press that lands on a node
    // goes to the node, not the canvas.
    let (resp, painter) =
        ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let center = resp.rect.center() - layout_center * zoom + *pan;

    // Stable key per node (contact id, or ME_KEY for the centre) and the screen
    // position of its auto-layout slot, before the drag offset. Offsets are
    // applied in a later pass so a group move can shift the whole selection at
    // once without lagging a frame behind.
    let keys: Vec<i64> = nodes
        .iter()
        .map(|n| match n.contact {
            Some(ci) => abos[ci].id,
            None => ME_KEY,
        })
        .collect();
    let base_screen: Vec<egui::Pos2> = base_pos.iter().map(|p| center + *p * zoom).collect();

    // Holding Ctrl turns any drag — on a node or on empty canvas — into a pan of
    // the whole view, instead of moving a node or drawing a rubber-band box.
    let ctrl = ui.input(|i| i.modifiers.ctrl);

    // The mouse wheel pans too (vertical, or horizontal with Shift), keeping the
    // chart navigable without dragging now that there is no scrollbar.
    if resp.hovered() {
        *pan += ui.input(|i| i.smooth_scroll_delta);
    }

    // Per-node drag interaction. Record at most one dragged node; its delta is
    // applied below (moves the node, or pans the view if Ctrl).
    let mut drag: Option<(usize, egui::Vec2)> = None;
    for i in 0..nodes.len() {
        let off = offsets.get(&keys[i]).copied().unwrap_or(egui::Vec2::ZERO);
        let pos = base_screen[i] + off;
        let rect = egui::Rect::from_center_size(pos, egui::Vec2::splat(node_r * 2.0));
        let node_resp =
            ui.interact(rect, egui::Id::new(("dln_node", keys[i])), egui::Sense::drag());
        if node_resp.dragged() {
            drag = Some((i, node_resp.drag_delta()));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if node_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    // Rubber-band rectangle to draw this frame (only while selecting; never
    // during a Ctrl-pan).
    let mut band: Option<egui::Rect> = None;

    if ctrl {
        // Pan: feed whichever drag is active — a press that landed on a node, or
        // one on empty canvas — into the view offset. Nodes don't move and the
        // selection is left untouched.
        let delta = drag
            .map(|(_, d)| d)
            .or_else(|| resp.dragged().then(|| resp.drag_delta()));
        if let Some(d) = delta {
            *pan += d;
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        *select_start = None;
    } else {
        // Apply a node drag: move the node, or the whole selection.
        if let Some((i, d)) = drag {
            if selected.contains(&keys[i]) {
                // Dragging a selected node moves the whole selection.
                for k in selected.iter() {
                    *offsets.entry(*k).or_insert(egui::Vec2::ZERO) += d;
                }
            } else {
                // Dragging an unselected node moves just it, and makes it the
                // sole selection.
                *offsets.entry(keys[i]).or_insert(egui::Vec2::ZERO) += d;
                selected.clear();
                selected.insert(keys[i]);
            }
        }

        // Rubber-band selection on the empty canvas.
        if resp.drag_started() {
            *select_start = resp.interact_pointer_pos();
            // Shift extends the current selection; otherwise start fresh.
            if !ui.input(|i| i.modifiers.shift) {
                selected.clear();
            }
        }
        band = match (*select_start, resp.interact_pointer_pos()) {
            (Some(start), Some(curr)) if resp.dragged() || resp.drag_stopped() => {
                Some(egui::Rect::from_two_pos(start, curr))
            }
            _ => None,
        };
        if resp.drag_stopped() {
            if let Some(rect) = band {
                for i in 0..nodes.len() {
                    let off = offsets.get(&keys[i]).copied().unwrap_or(egui::Vec2::ZERO);
                    if rect.contains(base_screen[i] + off) {
                        selected.insert(keys[i]);
                    }
                }
            }
            *select_start = None;
        }
        // A plain click on empty canvas clears the selection.
        if resp.clicked() {
            selected.clear();
        }
    }

    // Final positions (offsets now include any drag from this frame).
    for i in 0..nodes.len() {
        let off = offsets.get(&keys[i]).copied().unwrap_or(egui::Vec2::ZERO);
        nodes[i].pos = base_screen[i] + off;
    }

    // Edges first (so nodes draw on top).
    let edge = egui::Stroke::new(1.5 * zoom, egui::Color32::from_gray(170));
    for n in &nodes {
        for &child in &n.children {
            painter.line_segment([n.pos, nodes[child].pos], edge);
        }
    }

    // Nodes, with an amber ring on the selected ones.
    let ring_w = 3.0 * (node_r / 30.0).max(0.4);
    for i in 0..nodes.len() {
        draw_node(&painter, &nodes[i], &abos, node_r, &me_inside);
        if selected.contains(&keys[i]) {
            painter.circle_stroke(
                nodes[i].pos,
                node_r + 4.0,
                egui::Stroke::new(ring_w, SELECT_RING),
            );
        }
    }

    // The rubber-band rectangle on top while dragging.
    if let Some(rect) = band {
        painter.rect_filled(rect, 0.0, SELECT_RING.gamma_multiply(0.15));
        painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, SELECT_RING));
    }

    // Remember the chart's on-screen viewport so the export can crop to it.
    app.chart_export_rect = Some(resp.rect);
}

/// Recursively create nodes for `contact_idx` and its downline. Returns the new
/// node index, or `None` if this contact was already placed (cycle guard).
fn build_node(
    nodes: &mut Vec<Node>,
    abos: &[Contact],
    children_map: &HashMap<i64, Vec<usize>>,
    contact_idx: usize,
    depth: usize,
    visited: &mut HashSet<i64>,
) -> Option<usize> {
    let id = abos[contact_idx].id;
    if !visited.insert(id) {
        return None;
    }
    let my_idx = nodes.len();
    nodes.push(Node {
        contact: Some(contact_idx),
        depth,
        pos: egui::Pos2::ZERO,
        children: Vec::new(),
    });
    let kids = children_map.get(&id).cloned().unwrap_or_default();
    let mut child_nodes = Vec::new();
    for c in kids {
        if let Some(n) = build_node(nodes, abos, children_map, c, depth + 1, visited) {
            child_nodes.push(n);
        }
    }
    nodes[my_idx].children = child_nodes;
    Some(my_idx)
}

/// Smallest angular separation (radians) between any two nodes that sit on the
/// same `depth` ring. Returns `INFINITY` when fewer than two nodes share the
/// ring (then there is no angular spacing constraint for it).
fn min_angle_sep_at_depth(nodes: &[Node], angles: &[f32], depth: usize) -> f32 {
    let at: Vec<f32> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.depth == depth)
        .map(|(i, _)| angles[i])
        .collect();
    let mut best = f32::INFINITY;
    for i in 0..at.len() {
        for j in (i + 1)..at.len() {
            let mut d = (at[i] - at[j]).abs();
            if d > TAU - d {
                d = TAU - d; // take the shorter way around the circle
            }
            if d < best {
                best = d;
            }
        }
    }
    best
}

/// Post-order leaf count per node (used to size angular sectors).
fn compute_leaves(nodes: &[Node], idx: usize, leaves: &mut [usize]) -> usize {
    let kids = &nodes[idx].children;
    if kids.is_empty() {
        leaves[idx] = 1;
        return 1;
    }
    let mut sum = 0;
    for &ch in kids {
        sum += compute_leaves(nodes, ch, leaves);
    }
    let v = sum.max(1);
    leaves[idx] = v;
    v
}

/// Assign every node the mid-angle of its sector, giving each subtree an angular
/// span proportional to its leaf count. Radius (which ring) is decided
/// separately, so this only fixes direction from the centre.
fn assign_angles(nodes: &[Node], idx: usize, a0: f32, a1: f32, leaves: &[usize], angles: &mut [f32]) {
    angles[idx] = (a0 + a1) * 0.5;

    let kids = &nodes[idx].children;
    if kids.is_empty() {
        return;
    }
    let total: usize = kids.iter().map(|&c| leaves[c]).sum::<usize>().max(1);
    let mut a = a0;
    for &ch in kids {
        let span = (a1 - a0) * (leaves[ch] as f32 / total as f32);
        assign_angles(nodes, ch, a, a + span, leaves, angles);
        a += span;
    }
}

fn draw_node(painter: &egui::Painter, node: &Node, abos: &[Contact], r: f32, me_inside: &str) {
    let name_color = egui::Color32::from_gray(40);
    let (fill, inside_text, inside_color, below) = match node.contact {
        None => (
            ACCENT,
            me_inside.to_string(),
            egui::Color32::WHITE,
            "ฉัน (ME)".to_string(),
        ),
        Some(i) => {
            let c = &abos[i];
            let rank = c.rank.map(|rk| rk.as_str()).unwrap_or("ABO").to_string();
            (
                egui::Color32::from_rgb(0xB2, 0xEB, 0xF2),
                rank,
                ACCENT_STRONG,
                c.short_name(),
            )
        }
    };

    // Scale strokes / fonts with the node radius so they track the zoom level.
    let s = (r / 30.0).max(0.4);
    painter.circle_filled(node.pos, r, fill);
    painter.circle_stroke(node.pos, r, egui::Stroke::new(2.0 * s, ACCENT_STRONG));
    painter.text(
        node.pos,
        egui::Align2::CENTER_CENTER,
        inside_text,
        egui::FontId::proportional(12.0 * s),
        inside_color,
    );
    painter.text(
        node.pos + egui::vec2(0.0, r + 3.0 * s),
        egui::Align2::CENTER_TOP,
        below,
        egui::FontId::proportional(13.0 * s),
        name_color,
    );
}
