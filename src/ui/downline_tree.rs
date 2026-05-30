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
        if ui.button("จัดผังอัตโนมัติ (Auto-arrange)").clicked() {
            app.node_offsets.clear();
        }
        if ui
            .add(egui::Button::new("📊 ประเมินระดับของฉัน").fill(ACCENT))
            .on_hover_text("ประเมินระดับของฉัน (ME) จากดาวน์ไลน์ตรง + PPV")
            .clicked()
        {
            app.me_advisor = true;
        }
        ui.weak("ลาก node เพื่อย้าย • เลื่อนดูด้วยสกรอลล์ / ล้อเมาส์");
    });
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

    let node_r = 30.0_f32;

    // Choose ring spacing from the layout's own geometry: lay nodes out at unit
    // spacing, find the closest pair, then scale so even that pair clears
    // 2*node_r + gap. Because every position scales linearly with `ring`, this
    // yields the most compact layout that still avoids overlap — small networks
    // stay tight and fit one screen; dense ones grow (and gain scrollbars).
    assign_pos(&mut nodes, 0, 0.0, TAU, 1.0, egui::Pos2::ZERO, &leaves);
    let dmin = min_pair_dist(&nodes);
    let ring = ((2.0 * node_r + 40.0) / dmin).clamp(90.0, 800.0);

    // Size the canvas to the layout's actual bounding box (so a tree that leans
    // to one side doesn't force needless scroll), and centre the chart within
    // it. Small networks end up smaller than the viewport → no scrollbars.
    let mut min = egui::pos2(f32::MAX, f32::MAX);
    let mut max = egui::pos2(f32::MIN, f32::MIN);
    for n in &nodes {
        min.x = min.x.min(n.pos.x);
        min.y = min.y.min(n.pos.y);
        max.x = max.x.max(n.pos.x);
        max.y = max.y.max(n.pos.y);
    }
    let unit_center = egui::vec2((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
    let margin = node_r + 60.0;
    let avail = ui.available_size();
    let side_w = ((max.x - min.x) * ring + 2.0 * margin).max(avail.x);
    let side_h = ((max.y - min.y) * ring + 2.0 * margin).max(avail.y);

    let offsets = &mut app.node_offsets;

    egui::ScrollArea::both()
        .drag_to_scroll(false)
        .show(ui, |ui| {
            let (resp, painter) =
                ui.allocate_painter(egui::vec2(side_w, side_h), egui::Sense::hover());
            let center = resp.rect.center() - unit_center * ring;

            assign_pos(&mut nodes, 0, 0.0, TAU, ring, center, &leaves);

            // Apply stored offsets and let the user drag each node.
            for i in 0..nodes.len() {
                let key = match nodes[i].contact {
                    Some(ci) => abos[ci].id,
                    None => ME_KEY,
                };
                let base = nodes[i].pos;
                let off = offsets.get(&key).copied().unwrap_or(egui::Vec2::ZERO);
                let mut pos = base + off;

                let rect = egui::Rect::from_center_size(pos, egui::Vec2::splat(node_r * 2.0));
                let node_resp =
                    ui.interact(rect, egui::Id::new(("dln_node", key)), egui::Sense::drag());
                if node_resp.dragged() {
                    let d = node_resp.drag_delta();
                    *offsets.entry(key).or_insert(egui::Vec2::ZERO) += d;
                    pos += d;
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                } else if node_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                nodes[i].pos = pos;
            }

            // Edges first (so nodes draw on top).
            let edge = egui::Stroke::new(1.5, egui::Color32::from_gray(170));
            for n in &nodes {
                for &child in &n.children {
                    painter.line_segment([n.pos, nodes[child].pos], edge);
                }
            }

            // Nodes.
            for n in &nodes {
                draw_node(&painter, n, &abos, node_r, &me_inside);
            }
        });
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

/// Smallest distance between any two node centres at the current positions.
/// Used to scale ring spacing so nodes never overlap.
fn min_pair_dist(nodes: &[Node]) -> f32 {
    let mut dmin = f32::INFINITY;
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let d = nodes[i].pos.distance(nodes[j].pos);
            if d < dmin {
                dmin = d;
            }
        }
    }
    if dmin.is_finite() && dmin > 1e-3 {
        dmin
    } else {
        1.0
    }
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

/// Place each node at `depth * ring` from centre, giving every subtree an
/// angular sector proportional to its leaf count.
fn assign_pos(
    nodes: &mut [Node],
    idx: usize,
    a0: f32,
    a1: f32,
    ring: f32,
    center: egui::Pos2,
    leaves: &[usize],
) {
    let mid = (a0 + a1) * 0.5;
    let radius = nodes[idx].depth as f32 * ring;
    nodes[idx].pos = center + egui::Vec2::angled(mid) * radius;

    let kids = nodes[idx].children.clone();
    if kids.is_empty() {
        return;
    }
    let total: usize = kids.iter().map(|&c| leaves[c]).sum::<usize>().max(1);
    let mut a = a0;
    for ch in kids {
        let span = (a1 - a0) * (leaves[ch] as f32 / total as f32);
        assign_pos(nodes, ch, a, a + span, ring, center, leaves);
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
                c.display_name(),
            )
        }
    };

    painter.circle_filled(node.pos, r, fill);
    painter.circle_stroke(node.pos, r, egui::Stroke::new(2.0, ACCENT_STRONG));
    painter.text(
        node.pos,
        egui::Align2::CENTER_CENTER,
        inside_text,
        egui::FontId::proportional(12.0),
        inside_color,
    );
    painter.text(
        node.pos + egui::vec2(0.0, r + 3.0),
        egui::Align2::CENTER_TOP,
        below,
        egui::FontId::proportional(13.0),
        name_color,
    );
}
