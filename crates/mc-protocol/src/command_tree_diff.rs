//! Structural comparison of two `ClientboundCommandsPacket`s (tests + `mc-command-diff --tree`).
//!
//! Node indices differ between two servers (the entry order depends on which commands exist), so the packets are compared
//! as trees: per top-level command, node by node, addressed by path: name, flags, argument parser template, suggestion
//! provider, children order, redirect target (as a path). `a` is the reference (vanilla), `b` the candidate (ours).
use crate::packets::play::{
    COMMAND_NODE_REDIRECT, ClientboundCommandsEntry, ClientboundCommandsNodeStub, ClientboundCommandsPacket,
};
use std::collections::{HashMap, HashSet, VecDeque};

/// Commands whose children are compared as a set: vanilla registers `gamerule` literals in identity-hash order.
pub const UNORDERED: &[&str] = &["gamerule"];

/// At most this many differences are listed per command.
const MAX_DIFFS: usize = 25;

fn name_of(e: &ClientboundCommandsEntry) -> String {
    match &e.stub {
        Some(ClientboundCommandsNodeStub::Literal(n)) => n.clone(),
        Some(ClientboundCommandsNodeStub::Argument { id, .. }) => format!("<{id}>"),
        None => String::new(),
    }
}

fn sig(e: &ClientboundCommandsEntry) -> String {
    format!("{:?} flags={:#04x}", e.stub, e.flags)
}

/// `(name, entry index)` of the root's children, in packet order.
pub fn top_level(p: &ClientboundCommandsPacket) -> Vec<(String, usize)> {
    let Some(root) = p.entries.get(p.root_index as usize) else { return Vec::new() };
    root.children.iter().filter_map(|&c| p.entries.get(c as usize).map(|e| (name_of(e), c as usize))).collect()
}

fn entry(p: &ClientboundCommandsPacket, i: usize) -> Option<&ClientboundCommandsEntry> {
    p.entries.get(i)
}

/// First-visit DFS paths below `start` (child order).
fn dfs_paths(p: &ClientboundCommandsPacket, start: usize, start_path: &str) -> HashMap<usize, String> {
    let mut paths = HashMap::new();
    let mut stack = vec![(start, start_path.to_string())];
    // an explicit stack visits children in reverse; push them reversed to keep child order
    while let Some((i, path)) = stack.pop() {
        if paths.contains_key(&i) {
            continue;
        }
        paths.insert(i, path.clone());
        if let Some(e) = entry(p, i) {
            for &c in e.children.iter().rev() {
                if let Some(ce) = entry(p, c as usize) {
                    stack.push((c as usize, format!("{}/{}", path.trim_end_matches('/'), name_of(ce))));
                }
            }
        }
    }
    paths
}

/// Shortest paths from the root (breadth first): top-level commands are always `/name`.
fn bfs_paths(p: &ClientboundCommandsPacket) -> HashMap<usize, String> {
    let mut paths = HashMap::new();
    let mut queue = VecDeque::from([(p.root_index as usize, "/".to_string())]);
    while let Some((i, path)) = queue.pop_front() {
        if paths.contains_key(&i) {
            continue;
        }
        paths.insert(i, path.clone());
        if let Some(e) = entry(p, i) {
            for &c in &e.children {
                if let Some(ce) = entry(p, c as usize) {
                    queue.push_back((c as usize, format!("{}/{}", path.trim_end_matches('/'), name_of(ce))));
                }
            }
        }
    }
    paths
}

struct Side<'a> {
    p: &'a ClientboundCommandsPacket,
    local: HashMap<usize, String>,
    global: &'a HashMap<usize, String>,
}

impl Side<'_> {
    fn redirect_path(&self, e: &ClientboundCommandsEntry) -> String {
        if e.flags & COMMAND_NODE_REDIRECT == 0 {
            return String::new();
        }
        let t = e.redirect as usize;
        self.local.get(&t).or_else(|| self.global.get(&t)).cloned().unwrap_or_else(|| format!("#{t}"))
    }
}

fn cmp_node(a: &Side, ai: usize, b: &Side, bi: usize, path: &str, unordered: bool, seen: &mut HashSet<(usize, usize)>, out: &mut Vec<String>) {
    if out.len() >= MAX_DIFFS || !seen.insert((ai, bi)) {
        return;
    }
    let (Some(ea), Some(eb)) = (entry(a.p, ai), entry(b.p, bi)) else {
        out.push(format!("{path}: node index out of range"));
        return;
    };
    if sig(ea) != sig(eb) {
        out.push(format!("{path}: node differs\n    vanilla: {}\n    ours:    {}", sig(ea), sig(eb)));
    }
    let (ra, rb) = (a.redirect_path(ea), b.redirect_path(eb));
    if ra != rb {
        out.push(format!("{path}: redirect target differs\n    vanilla: {ra:?}\n    ours:    {rb:?}"));
    }
    let names = |s: &Side, e: &ClientboundCommandsEntry| -> Vec<(String, usize)> {
        e.children.iter().filter_map(|&c| entry(s.p, c as usize).map(|ce| (name_of(ce), c as usize))).collect()
    };
    let (ca, cb) = (names(a, ea), names(b, eb));
    let (na, nb): (Vec<&str>, Vec<&str>) = (ca.iter().map(|c| c.0.as_str()).collect(), cb.iter().map(|c| c.0.as_str()).collect());
    let same_names = if unordered {
        let (mut x, mut y) = (na.clone(), nb.clone());
        x.sort_unstable();
        y.sort_unstable();
        x == y
    } else {
        na == nb
    };
    if !same_names {
        let missing: Vec<&&str> = na.iter().filter(|n| !nb.contains(n)).collect();
        let extra: Vec<&&str> = nb.iter().filter(|n| !na.contains(n)).collect();
        let mut msg = format!("{path}: children differ");
        if !missing.is_empty() {
            msg += &format!("\n    missing in ours: {missing:?}");
        }
        if !extra.is_empty() {
            msg += &format!("\n    extra in ours:   {extra:?}");
        }
        if missing.is_empty() && extra.is_empty() {
            msg += &format!("\n    order (vanilla): {na:?}\n    order (ours):    {nb:?}");
        }
        out.push(msg);
    }
    for (n, ai2) in &ca {
        if let Some((_, bi2)) = cb.iter().find(|c| &c.0 == n) {
            let child_path = format!("{}/{}", path.trim_end_matches('/'), n);
            cmp_node(a, *ai2, b, *bi2, &child_path, unordered, seen, out);
        }
    }
}

/// Result of [`diff_trees`].
#[derive(Debug, Default)]
pub struct TreeReport {
    /// Top-level commands present in both and structurally equal.
    pub equal: Vec<String>,
    /// Top-level commands present in both whose subtrees differ, with the differing nodes.
    pub differing: Vec<(String, Vec<String>)>,
    /// Top-level commands only `a` (vanilla) has: not implemented yet.
    pub missing_in_b: Vec<String>,
    /// Top-level commands only `b` (ours) has: do not exist in vanilla at this permission level.
    pub extra_in_b: Vec<String>,
    /// Top-level command order differs (compared over the commands both have).
    pub order_differs: bool,
}

impl TreeReport {
    /// No differing subtree and no extra command (missing commands are only coverage).
    pub fn common_equal(&self) -> bool {
        self.differing.is_empty() && self.extra_in_b.is_empty()
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        for (name, diffs) in &self.differing {
            s += &format!("command `{name}` differs:\n");
            for d in diffs {
                s += &format!("  {d}\n");
            }
        }
        if !self.extra_in_b.is_empty() {
            s += &format!("commands only in ours (not in vanilla): {:?}\n", self.extra_in_b);
        }
        if self.order_differs {
            s += "top-level command order differs between vanilla and ours\n";
        }
        s += &format!(
            "{} command(s) equal, {} differ, {} not implemented yet\n",
            self.equal.len(),
            self.differing.len(),
            self.missing_in_b.len()
        );
        s
    }
}

/// Compares the subtree of every top-level command (`a` = vanilla, `b` = ours), see the module docs.
pub fn diff_trees(a: &ClientboundCommandsPacket, b: &ClientboundCommandsPacket) -> TreeReport {
    let (ga, gb) = (bfs_paths(a), bfs_paths(b));
    let (ta, tb) = (top_level(a), top_level(b));
    let mut report = TreeReport::default();
    for (name, ai) in &ta {
        let Some((_, bi)) = tb.iter().find(|(n, _)| n == name) else {
            report.missing_in_b.push(name.clone());
            continue;
        };
        let path = format!("/{name}");
        let (sa, sb) = (
            Side { p: a, local: dfs_paths(a, *ai, &path), global: &ga },
            Side { p: b, local: dfs_paths(b, *bi, &path), global: &gb },
        );
        let mut out = Vec::new();
        cmp_node(&sa, *ai, &sb, *bi, &path, UNORDERED.contains(&name.as_str()), &mut HashSet::new(), &mut out);
        if out.is_empty() {
            report.equal.push(name.clone());
        } else {
            report.differing.push((name.clone(), out));
        }
    }
    report.extra_in_b = tb.iter().filter(|(n, _)| !ta.iter().any(|(m, _)| m == n)).map(|(n, _)| n.clone()).collect();
    let common_a: Vec<&String> = ta.iter().map(|t| &t.0).filter(|n| tb.iter().any(|(m, _)| &m == n)).collect();
    let common_b: Vec<&String> = tb.iter().map(|t| &t.0).filter(|n| ta.iter().any(|(m, _)| &m == n)).collect();
    report.order_differs = common_a != common_b;
    report
}

/// Re-enumerates the packet exactly like `ClientboundCommandsPacket.enumerateNodes` (breadth first, children then
/// redirect) after sorting the children of the `UNORDERED` top-level commands by name: two packets that only differ in
/// those orders (and nothing else) get identical bytes.
pub fn canonical(p: &ClientboundCommandsPacket) -> ClientboundCommandsPacket {
    let sorted_children = |i: usize| -> Vec<i32> {
        let Some(e) = entry(p, i) else { return Vec::new() };
        let mut c = e.children.clone();
        let is_unordered = p.entries.get(p.root_index as usize).is_some_and(|r| {
            r.children.contains(&(i as i32)) && UNORDERED.contains(&name_of(e).as_str())
        });
        if is_unordered {
            c.sort_by_key(|&x| entry(p, x as usize).map(name_of).unwrap_or_default());
        }
        c
    };
    let mut ids: HashMap<usize, i32> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    let mut queue = VecDeque::from([p.root_index as usize]);
    while let Some(i) = queue.pop_front() {
        if ids.contains_key(&i) {
            continue;
        }
        ids.insert(i, order.len() as i32);
        order.push(i);
        let Some(e) = entry(p, i) else { continue };
        queue.extend(sorted_children(i).into_iter().map(|c| c as usize));
        if e.flags & COMMAND_NODE_REDIRECT != 0 {
            queue.push_back(e.redirect as usize);
        }
    }
    let map = |i: i32| ids.get(&(i as usize)).copied().unwrap_or(-1);
    let entries = order
        .iter()
        .filter_map(|&i| entry(p, i).map(|e| (i, e)))
        .map(|(i, e)| ClientboundCommandsEntry {
            stub: e.stub.clone(),
            flags: e.flags,
            redirect: if e.flags & COMMAND_NODE_REDIRECT != 0 { map(e.redirect) } else { 0 },
            children: sorted_children(i).into_iter().map(map).collect(),
        })
        .collect();
    ClientboundCommandsPacket { entries, root_index: 0 }
}
