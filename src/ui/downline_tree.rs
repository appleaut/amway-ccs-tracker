//! Network / downline tree: a recursive org chart of the ABO sponsor hierarchy.

use std::collections::{HashMap, HashSet};

use crate::app::AppState;
use crate::models::contact::Contact;

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("เครือข่าย / Downline Tree");
    ui.add_space(6.0);

    let r = app.db.list_abos();
    let abos = app.handle(r, Vec::new());
    if abos.is_empty() {
        ui.weak("ยังไม่มี ABO ในเครือข่าย — เพิ่มนักธุรกิจที่มีอัพไลน์เพื่อสร้างผังองค์กร");
        return;
    }

    // Build child adjacency by index. A node is a root if it has no sponsor, or
    // its sponsor is not itself an ABO in the set.
    let ids: HashSet<i64> = abos.iter().map(|c| c.id).collect();
    let mut children: HashMap<i64, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, c) in abos.iter().enumerate() {
        match c.sponsor_id {
            Some(sid) if ids.contains(&sid) => children.entry(sid).or_default().push(i),
            _ => roots.push(i),
        }
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut visited: HashSet<i64> = HashSet::new();
        for &root in &roots {
            render_node(ui, &abos, &children, root, &mut visited, 0);
        }
    });
}

fn render_node(
    ui: &mut egui::Ui,
    nodes: &[Contact],
    children: &HashMap<i64, Vec<usize>>,
    idx: usize,
    visited: &mut HashSet<i64>,
    depth: usize,
) {
    let node = &nodes[idx];
    // Guard against cycles and runaway depth.
    if !visited.insert(node.id) || depth > 32 {
        return;
    }

    let rank = node.rank.map(|r| r.as_str()).unwrap_or("ABO");
    let label = format!("{}  [{}]", node.display_name(), rank);

    match children.get(&node.id) {
        Some(kids) if !kids.is_empty() => {
            egui::CollapsingHeader::new(label)
                .id_source(node.id)
                .default_open(true)
                .show(ui, |ui| {
                    for &child in kids {
                        render_node(ui, nodes, children, child, visited, depth + 1);
                    }
                });
        }
        _ => {
            ui.label(format!("• {label}"));
        }
    }
}
