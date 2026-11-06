//! `net.minecraft.advancements.TreeNodePosition`: the Reingold-Tilford style layout of an advancement tab
//! (positions are `float` exactly like vanilla).

use super::advancement_tree::{AdvancementTree, NodeId};

struct Pos {
    node: NodeId,
    parent: Option<usize>,
    previous_sibling: Option<usize>,
    child_index: i32,
    children: Vec<usize>,
    ancestor: usize,
    thread: Option<usize>,
    x: i32,
    y: f32,
    mod_: f32,
    change: f32,
    shift: f32,
}

struct Layout<'a> {
    tree: &'a AdvancementTree,
    p: Vec<Pos>,
}

impl Layout<'_> {
    /// The constructor: positions `node` and (recursively) its visible descendants.
    fn create(
        &mut self,
        node: NodeId,
        parent: Option<usize>,
        previous_sibling: Option<usize>,
        child_index: i32,
        depth: i32,
    ) -> usize {
        assert!(
            self.tree.node(node).advancement().display.is_some(),
            "Can't position an invisible advancement!"
        );
        let me = self.p.len();
        self.p.push(Pos {
            node,
            parent,
            previous_sibling,
            child_index,
            children: Vec::new(),
            ancestor: me,
            thread: None,
            x: depth,
            y: -1.0,
            mod_: 0.0,
            change: 0.0,
            shift: 0.0,
        });
        let mut previous = None;
        for child in self.tree.node(node).children.clone() {
            previous = self.add_child(me, child, previous);
        }
        me
    }

    fn add_child(&mut self, me: usize, node: NodeId, mut previous: Option<usize>) -> Option<usize> {
        if self.tree.node(node).advancement().display.is_some() {
            let child = self.create(
                node,
                Some(me),
                previous,
                self.p[me].children.len() as i32 + 1,
                self.p[me].x + 1,
            );
            self.p[me].children.push(child);
            previous = Some(child);
        } else {
            for grandchild in self.tree.node(node).children.clone() {
                previous = self.add_child(me, grandchild, previous);
            }
        }
        previous
    }

    fn first_walk(&mut self, me: usize) {
        if self.p[me].children.is_empty() {
            self.p[me].y = match self.p[me].previous_sibling {
                Some(prev) => self.p[prev].y + 1.0,
                None => 0.0,
            };
            return;
        }
        let mut default_ancestor: Option<usize> = None;
        for child in self.p[me].children.clone() {
            self.first_walk(child);
            default_ancestor = Some(self.apportion(child, default_ancestor.unwrap_or(child)));
        }
        self.execute_shifts(me);
        let children = &self.p[me].children;
        let midpoint = (self.p[children[0]].y + self.p[children[children.len() - 1]].y) / 2.0;
        match self.p[me].previous_sibling {
            Some(prev) => {
                self.p[me].y = self.p[prev].y + 1.0;
                self.p[me].mod_ = self.p[me].y - midpoint;
            }
            None => self.p[me].y = midpoint,
        }
    }

    fn second_walk(&mut self, me: usize, mod_sum: f32, depth: i32, mut min: f32) -> f32 {
        self.p[me].y += mod_sum;
        self.p[me].x = depth;
        if self.p[me].y < min {
            min = self.p[me].y;
        }
        let child_mod = mod_sum + self.p[me].mod_;
        for child in self.p[me].children.clone() {
            min = self.second_walk(child, child_mod, depth + 1, min);
        }
        min
    }

    fn third_walk(&mut self, me: usize, offset: f32) {
        self.p[me].y += offset;
        for child in self.p[me].children.clone() {
            self.third_walk(child, offset);
        }
    }

    fn execute_shifts(&mut self, me: usize) {
        let mut shift = 0.0f32;
        let mut change = 0.0f32;
        for i in (0..self.p[me].children.len()).rev() {
            let child = self.p[me].children[i];
            self.p[child].y += shift;
            self.p[child].mod_ += shift;
            change += self.p[child].change;
            shift += self.p[child].shift + change;
        }
    }

    fn previous_or_thread(&self, me: usize) -> Option<usize> {
        match self.p[me].thread {
            Some(t) => Some(t),
            None => self.p[me].children.first().copied(),
        }
    }

    fn next_or_thread(&self, me: usize) -> Option<usize> {
        match self.p[me].thread {
            Some(t) => Some(t),
            None => self.p[me].children.last().copied(),
        }
    }

    fn apportion(&mut self, me: usize, mut default_ancestor: usize) -> usize {
        let Some(previous_sibling) = self.p[me].previous_sibling else {
            return default_ancestor;
        };
        let mut vir = me;
        let mut vor = me;
        let mut vil = previous_sibling;
        let parent = self.p[me]
            .parent
            .expect("a node with a previous sibling has a parent");
        let mut vol = self.p[parent].children[0];
        let mut sir = self.p[me].mod_;
        let mut sor = self.p[me].mod_;
        let mut sil = self.p[vil].mod_;
        let mut sol = self.p[vol].mod_;
        while self.next_or_thread(vil).is_some() && self.previous_or_thread(vir).is_some() {
            vil = self.next_or_thread(vil).unwrap();
            vir = self.previous_or_thread(vir).unwrap();
            vol = self.previous_or_thread(vol).unwrap();
            vor = self.next_or_thread(vor).unwrap();
            self.p[vor].ancestor = me;
            let shift = self.p[vil].y + sil - (self.p[vir].y + sir) + 1.0;
            if shift > 0.0 {
                let ancestor = self.get_ancestor(vil, me, default_ancestor);
                self.move_subtree(ancestor, me, shift);
                sir += shift;
                sor += shift;
            }
            sil += self.p[vil].mod_;
            sir += self.p[vir].mod_;
            sol += self.p[vol].mod_;
            sor += self.p[vor].mod_;
        }
        if self.next_or_thread(vil).is_some() && self.next_or_thread(vor).is_none() {
            self.p[vor].thread = self.next_or_thread(vil);
            self.p[vor].mod_ += sil - sor;
        } else {
            if self.previous_or_thread(vir).is_some() && self.previous_or_thread(vol).is_none() {
                self.p[vol].thread = self.previous_or_thread(vir);
                self.p[vol].mod_ += sir - sol;
            }
            default_ancestor = me;
        }
        default_ancestor
    }

    fn move_subtree(&mut self, left: usize, right: usize, shift: f32) {
        let subtrees = (self.p[right].child_index - self.p[left].child_index) as f32;
        if subtrees != 0.0 {
            self.p[right].change -= shift / subtrees;
            self.p[left].change += shift / subtrees;
        }
        self.p[right].shift += shift;
        self.p[right].y += shift;
        self.p[right].mod_ += shift;
    }

    /// `this.getAncestor(other, defaultAncestor)` with `this` = `me`, `other` = `other`.
    fn get_ancestor(&self, me: usize, other: usize, default_ancestor: usize) -> usize {
        let ancestor = self.p[me].ancestor;
        let other_parent = self.p[other]
            .parent
            .expect("a node with a previous sibling has a parent");
        if self.p[other_parent].children.contains(&ancestor) {
            ancestor
        } else {
            default_ancestor
        }
    }
}

/// `TreeNodePosition.run(root)`: stores the final `x`/`y` in the tree's nodes.
pub fn run(tree: &mut AdvancementTree, root: NodeId) {
    assert!(
        tree.node(root).advancement().display.is_some(),
        "Can't position children of an invisible root!"
    );
    let (layout, _root_pos) = {
        let mut layout = Layout {
            tree,
            p: Vec::new(),
        };
        let root_pos = layout.create(root, None, None, 1, 0);
        layout.first_walk(root_pos);
        let min = layout.second_walk(root_pos, 0.0, 0, layout.p[root_pos].y);
        if min < 0.0 {
            layout.third_walk(root_pos, -min);
        }
        (layout.p, root_pos)
    };
    for pos in &layout {
        tree.set_location(pos.node, pos.x as f32, pos.y);
    }
}
