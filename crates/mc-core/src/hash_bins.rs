//! `java.util.HashMap<String, V>` with its bins, tree bins included: the iteration order of a map after a sequence of
//! `put`s and `remove`s, where a bin of 9 or more keys in a table of 64 or more buckets is a red-black tree
//! (`HashMap.TreeNode`) whose `next` chain, which the iterator walks, is not the insertion order.
//!
//! Only the keys are modelled (values stay with the caller, keyed by the key's index). The code follows the JDK's
//! `HashMap` (JDK 25): `putVal`, `resize` (with `TreeNode.split`), `treeifyBin`, `TreeNode.treeify`, `putTreeVal`,
//! `removeTreeNode`, the red-black balancing and `moveRootToFront`. Keys are `String`s: two keys that differ always
//! compare unequal, so `tieBreakOrder` (an identity hash) is never reached.

use std::cmp::Ordering;

const NIL: usize = usize::MAX;
const MIN_TREEIFY_CAPACITY: usize = 64;
const UNTREEIFY_THRESHOLD: usize = 6;

#[derive(Clone)]
struct Node {
    hash: i32,
    key: usize,
    next: usize,
    prev: usize,
    parent: usize,
    left: usize,
    right: usize,
    red: bool,
    /// An instance of `TreeNode` (the whole bin is a tree).
    tree: bool,
}

/// The table of one `HashMap` over the keys `0..keys`: `hash(k)` is `String.hashCode()` of key `k` and `compare` is
/// `String.compareTo`.
pub struct Bins<'a> {
    hashes: Vec<i32>,
    compare: &'a dyn Fn(usize, usize) -> Ordering,
    nodes: Vec<Node>,
    /// The node of each key that is in the map (`NIL` otherwise).
    node_of: Vec<usize>,
    table: Vec<usize>,
    size: usize,
    /// `HashMap.threshold`: the initial capacity until the table exists.
    threshold: usize,
}

fn spread(h: i32) -> i32 {
    h ^ ((h as u32) >> 16) as i32
}

impl<'a> Bins<'a> {
    /// A map without a table whose first `resize` makes `initial_capacity` buckets (`0`: the default 16).
    pub fn new(
        hash_codes: Vec<i32>,
        compare: &'a dyn Fn(usize, usize) -> Ordering,
        initial_capacity: usize,
    ) -> Self {
        Bins {
            node_of: vec![NIL; hash_codes.len()],
            hashes: hash_codes.into_iter().map(spread).collect(),
            compare,
            nodes: Vec::new(),
            table: Vec::new(),
            size: 0,
            threshold: initial_capacity,
        }
    }

    /// `put(key, ..)` of a key that is not in the map yet; a key that is has its value replaced and nothing else changes.
    pub fn put(&mut self, key: usize) {
        if self.node_of[key] != NIL {
            return;
        }
        let hash = self.hashes[key];
        if self.table.is_empty() {
            self.resize();
        }
        let n = self.table.len();
        let i = (n - 1) & hash as u32 as usize;
        let p = self.table[i];
        if p == NIL {
            self.table[i] = self.new_node(key, NIL);
        } else if self.nodes[p].tree {
            self.put_tree_val(p, key);
        } else {
            let mut p = p;
            let mut bin_count = 0;
            loop {
                let e = self.nodes[p].next;
                if e == NIL {
                    let node = self.new_node(key, NIL);
                    self.nodes[p].next = node;
                    if bin_count >= 7 {
                        self.treeify_bin(hash);
                    }
                    break;
                }
                p = e;
                bin_count += 1;
            }
        }
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
    }

    /// `remove(key)`.
    pub fn remove(&mut self, key: usize) {
        let node = self.node_of[key];
        if node == NIL {
            return;
        }
        let index = (self.table.len() - 1) & self.hashes[key] as u32 as usize;
        if self.nodes[node].tree {
            self.remove_tree_node(node);
        } else {
            let first = self.table[index];
            if first == node {
                self.table[index] = self.nodes[node].next;
            } else {
                let mut p = first;
                while self.nodes[p].next != node {
                    p = self.nodes[p].next;
                }
                self.nodes[p].next = self.nodes[node].next;
            }
        }
        self.node_of[key] = NIL;
        self.size -= 1;
    }

    /// The number of buckets of the table (`0` before the first `put`).
    pub fn capacity(&self) -> usize {
        self.table.len()
    }

    /// The keys in iteration order: buckets in ascending order, each bin along its `next` chain.
    pub fn order(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.size);
        for &head in &self.table {
            let mut e = head;
            while e != NIL {
                out.push(self.nodes[e].key);
                e = self.nodes[e].next;
            }
        }
        out
    }

    fn new_node(&mut self, key: usize, next: usize) -> usize {
        self.nodes.push(Node {
            hash: self.hashes[key],
            key,
            next,
            prev: NIL,
            parent: NIL,
            left: NIL,
            right: NIL,
            red: false,
            tree: false,
        });
        self.node_of[key] = self.nodes.len() - 1;
        self.nodes.len() - 1
    }

    fn index(&self, hash: i32) -> usize {
        (self.table.len() - 1) & hash as u32 as usize
    }

    /// `resize()`.
    fn resize(&mut self) {
        let old_cap = self.table.len();
        let old_thr = self.threshold;
        let (new_cap, mut new_thr) = if old_cap > 0 {
            (old_cap << 1, if old_cap >= 16 { old_thr << 1 } else { 0 })
        } else if old_thr > 0 {
            (old_thr, 0)
        } else {
            (16, 12)
        };
        if new_thr == 0 {
            new_thr = (new_cap as f32 * 0.75f32) as usize;
        }
        self.threshold = new_thr;
        let old = std::mem::replace(&mut self.table, vec![NIL; new_cap]);
        for (j, &head) in old.iter().enumerate() {
            let mut e = head;
            if e == NIL {
                continue;
            }
            if self.nodes[e].next == NIL {
                let at = self.index(self.nodes[e].hash);
                self.table[at] = e;
            } else if self.nodes[e].tree {
                self.split(e, j, old_cap);
            } else {
                let (mut lo_head, mut lo_tail, mut hi_head, mut hi_tail) = (NIL, NIL, NIL, NIL);
                while e != NIL {
                    let next = self.nodes[e].next;
                    if self.nodes[e].hash as u32 as usize & old_cap == 0 {
                        if lo_tail == NIL {
                            lo_head = e;
                        } else {
                            self.nodes[lo_tail].next = e;
                        }
                        lo_tail = e;
                    } else {
                        if hi_tail == NIL {
                            hi_head = e;
                        } else {
                            self.nodes[hi_tail].next = e;
                        }
                        hi_tail = e;
                    }
                    e = next;
                }
                if lo_tail != NIL {
                    self.nodes[lo_tail].next = NIL;
                    self.table[j] = lo_head;
                }
                if hi_tail != NIL {
                    self.nodes[hi_tail].next = NIL;
                    self.table[j + old_cap] = hi_head;
                }
            }
        }
    }

    /// `treeifyBin(tab, hash)`.
    fn treeify_bin(&mut self, hash: i32) {
        if self.table.len() < MIN_TREEIFY_CAPACITY {
            self.resize();
            return;
        }
        let index = self.index(hash);
        let mut e = self.table[index];
        if e == NIL {
            return;
        }
        let (mut hd, mut tl) = (NIL, NIL);
        while e != NIL {
            let next = self.nodes[e].next;
            let node = &mut self.nodes[e];
            (node.tree, node.red) = (true, false);
            (node.parent, node.left, node.right) = (NIL, NIL, NIL);
            node.prev = tl;
            node.next = NIL;
            if tl == NIL {
                hd = e;
            } else {
                self.nodes[tl].next = e;
            }
            tl = e;
            e = next;
        }
        self.table[index] = hd;
        self.treeify(hd);
    }

    /// `TreeNode.treeify(tab)`: the tree of the nodes linked from `head`.
    fn treeify(&mut self, head: usize) {
        let mut root = NIL;
        let mut x = head;
        while x != NIL {
            let next = self.nodes[x].next;
            self.nodes[x].left = NIL;
            self.nodes[x].right = NIL;
            if root == NIL {
                self.nodes[x].parent = NIL;
                self.nodes[x].red = false;
                root = x;
            } else {
                let mut p = root;
                loop {
                    let dir = self.direction(x, p);
                    let xp = p;
                    p = if dir <= 0 {
                        self.nodes[xp].left
                    } else {
                        self.nodes[xp].right
                    };
                    if p == NIL {
                        self.nodes[x].parent = xp;
                        if dir <= 0 {
                            self.nodes[xp].left = x;
                        } else {
                            self.nodes[xp].right = x;
                        }
                        root = self.balance_insertion(root, x);
                        break;
                    }
                }
            }
            x = next;
        }
        self.move_root_to_front(root);
    }

    /// Where the key of `x` goes below `p`: by hash, then by `compareTo` (distinct strings never compare equal).
    fn direction(&self, x: usize, p: usize) -> i32 {
        let (h, ph) = (self.nodes[x].hash, self.nodes[p].hash);
        match ph.cmp(&h) {
            Ordering::Greater => -1,
            Ordering::Less => 1,
            Ordering::Equal => match (self.compare)(self.nodes[x].key, self.nodes[p].key) {
                Ordering::Less => -1,
                _ => 1,
            },
        }
    }

    fn root_of(&self, mut r: usize) -> usize {
        while self.nodes[r].parent != NIL {
            r = self.nodes[r].parent;
        }
        r
    }

    /// `TreeNode.moveRootToFront(tab, root)`.
    fn move_root_to_front(&mut self, root: usize) {
        if root == NIL || self.table.is_empty() {
            return;
        }
        let index = self.index(self.nodes[root].hash);
        let first = self.table[index];
        if root != first {
            self.table[index] = root;
            let rp = self.nodes[root].prev;
            let rn = self.nodes[root].next;
            if rn != NIL {
                self.nodes[rn].prev = rp;
            }
            if rp != NIL {
                self.nodes[rp].next = rn;
            }
            if first != NIL {
                self.nodes[first].prev = root;
            }
            self.nodes[root].next = first;
            self.nodes[root].prev = NIL;
        }
    }

    /// `TreeNode.putTreeVal` for a key that is not in the tree.
    fn put_tree_val(&mut self, head: usize, key: usize) {
        let root = self.root_of(head);
        let mut p = root;
        let x = self.new_node(key, NIL);
        self.nodes[x].tree = true;
        loop {
            let dir = self.direction(x, p);
            let xp = p;
            p = if dir <= 0 {
                self.nodes[xp].left
            } else {
                self.nodes[xp].right
            };
            if p == NIL {
                let xpn = self.nodes[xp].next;
                self.nodes[x].next = xpn;
                if dir <= 0 {
                    self.nodes[xp].left = x;
                } else {
                    self.nodes[xp].right = x;
                }
                self.nodes[xp].next = x;
                self.nodes[x].parent = xp;
                self.nodes[x].prev = xp;
                if xpn != NIL {
                    self.nodes[xpn].prev = x;
                }
                let root = self.balance_insertion(root, x);
                self.move_root_to_front(root);
                return;
            }
        }
    }

    /// `TreeNode.untreeify`: the bin from `first` on as plain nodes, in the same order.
    fn untreeify(&mut self, first: usize) -> usize {
        let mut q = first;
        while q != NIL {
            let node = &mut self.nodes[q];
            (node.tree, node.red) = (false, false);
            (node.parent, node.left, node.right, node.prev) = (NIL, NIL, NIL, NIL);
            q = node.next;
        }
        first
    }

    /// `TreeNode.split(map, tab, index, bit)`.
    fn split(&mut self, b: usize, index: usize, bit: usize) {
        let (mut lo_head, mut lo_tail, mut hi_head, mut hi_tail) = (NIL, NIL, NIL, NIL);
        let (mut lc, mut hc) = (0, 0);
        let mut e = b;
        while e != NIL {
            let next = self.nodes[e].next;
            self.nodes[e].next = NIL;
            if self.nodes[e].hash as u32 as usize & bit == 0 {
                self.nodes[e].prev = lo_tail;
                if lo_tail == NIL {
                    lo_head = e;
                } else {
                    self.nodes[lo_tail].next = e;
                }
                lo_tail = e;
                lc += 1;
            } else {
                self.nodes[e].prev = hi_tail;
                if hi_tail == NIL {
                    hi_head = e;
                } else {
                    self.nodes[hi_tail].next = e;
                }
                hi_tail = e;
                hc += 1;
            }
            e = next;
        }
        if lo_head != NIL {
            if lc <= UNTREEIFY_THRESHOLD {
                self.table[index] = self.untreeify(lo_head);
            } else {
                self.table[index] = lo_head;
                if hi_head != NIL {
                    self.treeify(lo_head);
                }
            }
        }
        if hi_head != NIL {
            if hc <= UNTREEIFY_THRESHOLD {
                self.table[index + bit] = self.untreeify(hi_head);
            } else {
                self.table[index + bit] = hi_head;
                if lo_head != NIL {
                    self.treeify(hi_head);
                }
            }
        }
    }

    /// `TreeNode.removeTreeNode(map, tab, true)` for node `p`.
    fn remove_tree_node(&mut self, p: usize) {
        let index = self.index(self.nodes[p].hash);
        let mut first = self.table[index];
        let mut root = first;
        let succ = self.nodes[p].next;
        let pred = self.nodes[p].prev;
        if pred == NIL {
            self.table[index] = succ;
            first = succ;
        } else {
            self.nodes[pred].next = succ;
        }
        if succ != NIL {
            self.nodes[succ].prev = pred;
        }
        if first == NIL {
            return;
        }
        if self.nodes[root].parent != NIL {
            root = self.root_of(root);
        }
        let too_small = {
            let r = &self.nodes[root];
            r.right == NIL || r.left == NIL || self.nodes[r.left].left == NIL
        };
        if too_small {
            let head = self.untreeify(first);
            self.table[index] = head;
            return;
        }
        let (pl, pr) = (self.nodes[p].left, self.nodes[p].right);
        let replacement;
        if pl != NIL && pr != NIL {
            let mut s = pr;
            while self.nodes[s].left != NIL {
                s = self.nodes[s].left;
            }
            let c = self.nodes[s].red;
            self.nodes[s].red = self.nodes[p].red;
            self.nodes[p].red = c;
            let sr = self.nodes[s].right;
            let pp = self.nodes[p].parent;
            if s == pr {
                self.nodes[p].parent = s;
                self.nodes[s].right = p;
            } else {
                let sp = self.nodes[s].parent;
                self.nodes[p].parent = sp;
                if sp != NIL {
                    if s == self.nodes[sp].left {
                        self.nodes[sp].left = p;
                    } else {
                        self.nodes[sp].right = p;
                    }
                }
                self.nodes[s].right = pr;
                if pr != NIL {
                    self.nodes[pr].parent = s;
                }
            }
            self.nodes[p].left = NIL;
            self.nodes[p].right = sr;
            if sr != NIL {
                self.nodes[sr].parent = p;
            }
            self.nodes[s].left = pl;
            if pl != NIL {
                self.nodes[pl].parent = s;
            }
            self.nodes[s].parent = pp;
            if pp == NIL {
                root = s;
            } else if p == self.nodes[pp].left {
                self.nodes[pp].left = s;
            } else {
                self.nodes[pp].right = s;
            }
            replacement = if sr != NIL { sr } else { p };
        } else if pl != NIL {
            replacement = pl;
        } else if pr != NIL {
            replacement = pr;
        } else {
            replacement = p;
        }
        if replacement != p {
            let pp = self.nodes[p].parent;
            self.nodes[replacement].parent = pp;
            if pp == NIL {
                root = replacement;
                self.nodes[root].red = false;
            } else if p == self.nodes[pp].left {
                self.nodes[pp].left = replacement;
            } else {
                self.nodes[pp].right = replacement;
            }
            self.nodes[p].left = NIL;
            self.nodes[p].right = NIL;
            self.nodes[p].parent = NIL;
        }
        let r = if self.nodes[p].red {
            root
        } else {
            self.balance_deletion(root, replacement)
        };
        if replacement == p {
            let pp = self.nodes[p].parent;
            self.nodes[p].parent = NIL;
            if pp != NIL {
                if p == self.nodes[pp].left {
                    self.nodes[pp].left = NIL;
                } else if p == self.nodes[pp].right {
                    self.nodes[pp].right = NIL;
                }
            }
        }
        self.move_root_to_front(r);
    }

    fn rotate_left(&mut self, mut root: usize, p: usize) -> usize {
        let r = if p != NIL { self.nodes[p].right } else { NIL };
        if r != NIL {
            let rl = self.nodes[r].left;
            self.nodes[p].right = rl;
            if rl != NIL {
                self.nodes[rl].parent = p;
            }
            let pp = self.nodes[p].parent;
            self.nodes[r].parent = pp;
            if pp == NIL {
                root = r;
                self.nodes[r].red = false;
            } else if self.nodes[pp].left == p {
                self.nodes[pp].left = r;
            } else {
                self.nodes[pp].right = r;
            }
            self.nodes[r].left = p;
            self.nodes[p].parent = r;
        }
        root
    }

    fn rotate_right(&mut self, mut root: usize, p: usize) -> usize {
        let l = if p != NIL { self.nodes[p].left } else { NIL };
        if l != NIL {
            let lr = self.nodes[l].right;
            self.nodes[p].left = lr;
            if lr != NIL {
                self.nodes[lr].parent = p;
            }
            let pp = self.nodes[p].parent;
            self.nodes[l].parent = pp;
            if pp == NIL {
                root = l;
                self.nodes[l].red = false;
            } else if self.nodes[pp].right == p {
                self.nodes[pp].right = l;
            } else {
                self.nodes[pp].left = l;
            }
            self.nodes[l].right = p;
            self.nodes[p].parent = l;
        }
        root
    }

    fn parent_of(&self, x: usize) -> usize {
        if x == NIL { NIL } else { self.nodes[x].parent }
    }

    fn balance_insertion(&mut self, mut root: usize, mut x: usize) -> usize {
        self.nodes[x].red = true;
        loop {
            let mut xp = self.nodes[x].parent;
            if xp == NIL {
                self.nodes[x].red = false;
                return x;
            }
            let mut xpp = self.nodes[xp].parent;
            if !self.nodes[xp].red || xpp == NIL {
                return root;
            }
            let xppl = self.nodes[xpp].left;
            if xp == xppl {
                let xppr = self.nodes[xpp].right;
                if xppr != NIL && self.nodes[xppr].red {
                    self.nodes[xppr].red = false;
                    self.nodes[xp].red = false;
                    self.nodes[xpp].red = true;
                    x = xpp;
                } else {
                    if x == self.nodes[xp].right {
                        x = xp;
                        root = self.rotate_left(root, x);
                        xp = self.parent_of(x);
                        xpp = self.parent_of(xp);
                    }
                    if xp != NIL {
                        self.nodes[xp].red = false;
                        if xpp != NIL {
                            self.nodes[xpp].red = true;
                            root = self.rotate_right(root, xpp);
                        }
                    }
                }
            } else if xppl != NIL && self.nodes[xppl].red {
                self.nodes[xppl].red = false;
                self.nodes[xp].red = false;
                self.nodes[xpp].red = true;
                x = xpp;
            } else {
                if x == self.nodes[xp].left {
                    x = xp;
                    root = self.rotate_right(root, x);
                    xp = self.parent_of(x);
                    xpp = self.parent_of(xp);
                }
                if xp != NIL {
                    self.nodes[xp].red = false;
                    if xpp != NIL {
                        self.nodes[xpp].red = true;
                        root = self.rotate_left(root, xpp);
                    }
                }
            }
        }
    }

    fn is_red(&self, x: usize) -> bool {
        x != NIL && self.nodes[x].red
    }

    fn balance_deletion(&mut self, mut root: usize, mut x: usize) -> usize {
        loop {
            if x == NIL || x == root {
                return root;
            }
            let mut xp = self.nodes[x].parent;
            if xp == NIL {
                self.nodes[x].red = false;
                return x;
            }
            if self.nodes[x].red {
                self.nodes[x].red = false;
                return root;
            }
            let xpl = self.nodes[xp].left;
            if xpl == x {
                let mut xpr = self.nodes[xp].right;
                if self.is_red(xpr) {
                    self.nodes[xpr].red = false;
                    self.nodes[xp].red = true;
                    root = self.rotate_left(root, xp);
                    xp = self.nodes[x].parent;
                    xpr = if xp == NIL { NIL } else { self.nodes[xp].right };
                }
                if xpr == NIL {
                    x = xp;
                } else {
                    let (sl, mut sr) = (self.nodes[xpr].left, self.nodes[xpr].right);
                    if !self.is_red(sr) && !self.is_red(sl) {
                        self.nodes[xpr].red = true;
                        x = xp;
                    } else {
                        if !self.is_red(sr) {
                            if sl != NIL {
                                self.nodes[sl].red = false;
                            }
                            self.nodes[xpr].red = true;
                            root = self.rotate_right(root, xpr);
                            xp = self.nodes[x].parent;
                            xpr = if xp == NIL { NIL } else { self.nodes[xp].right };
                        }
                        if xpr != NIL {
                            self.nodes[xpr].red = xp != NIL && self.nodes[xp].red;
                            sr = self.nodes[xpr].right;
                            if sr != NIL {
                                self.nodes[sr].red = false;
                            }
                        }
                        if xp != NIL {
                            self.nodes[xp].red = false;
                            root = self.rotate_left(root, xp);
                        }
                        x = root;
                    }
                }
            } else {
                let mut xpl = xpl;
                if self.is_red(xpl) {
                    self.nodes[xpl].red = false;
                    self.nodes[xp].red = true;
                    root = self.rotate_right(root, xp);
                    xp = self.nodes[x].parent;
                    xpl = if xp == NIL { NIL } else { self.nodes[xp].left };
                }
                if xpl == NIL {
                    x = xp;
                } else {
                    let (mut sl, sr) = (self.nodes[xpl].left, self.nodes[xpl].right);
                    if !self.is_red(sl) && !self.is_red(sr) {
                        self.nodes[xpl].red = true;
                        x = xp;
                    } else {
                        if !self.is_red(sl) {
                            if sr != NIL {
                                self.nodes[sr].red = false;
                            }
                            self.nodes[xpl].red = true;
                            root = self.rotate_left(root, xpl);
                            xp = self.nodes[x].parent;
                            xpl = if xp == NIL { NIL } else { self.nodes[xp].left };
                        }
                        if xpl != NIL {
                            self.nodes[xpl].red = xp != NIL && self.nodes[xp].red;
                            sl = self.nodes[xpl].left;
                            if sl != NIL {
                                self.nodes[sl].red = false;
                            }
                        }
                        if xp != NIL {
                            self.nodes[xp].red = false;
                            root = self.rotate_right(root, xp);
                        }
                        x = root;
                    }
                }
            }
        }
    }
}
