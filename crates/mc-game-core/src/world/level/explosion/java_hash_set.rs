//! `java.util.HashSet<BlockPos>` with Java's exact iteration order, including treeified bins.
//!
//! `calculateExplodedPositions` collects up to thousands of positions whose `Vec3i.hashCode()` (`31*(31*z+y)+x`) collides
//! heavily, so bins reach the treeify threshold; the iteration order of a treeified bin follows the `next` links that
//! `TreeNode.treeify` / `putTreeVal` / `moveRootToFront` / `split` maintain, and that order feeds `Util.shuffle` (drops, fire).
//! This is a port of `HashMap.putVal` + `TreeNode` (no removal). Keys of equal spread hash are never ordered by
//! `tieBreakOrder` (identity hash codes, nondeterministic in vanilla); positions of one explosion cannot collide on the full
//! hash, a collision would be a bug here and panics.

use mc_core::pos::BlockPos;

const NIL: usize = usize::MAX;
const TREEIFY_THRESHOLD: usize = 8;
const UNTREEIFY_THRESHOLD: usize = 6;
const MIN_TREEIFY_CAPACITY: usize = 64;

#[derive(Clone)]
struct Node {
    hash: i32,
    key: BlockPos,
    next: usize,
    prev: usize,
    parent: usize,
    left: usize,
    right: usize,
    red: bool,
}

pub struct JavaBlockPosHashSet {
    nodes: Vec<Node>,
    table: Vec<usize>,
    tree: Vec<bool>,
    size: usize,
    threshold: usize,
}

/// `HashMap.hash(key)` of `Vec3i.hashCode()`.
fn spread(pos: &BlockPos) -> i32 {
    let h = pos.y().wrapping_add(pos.z().wrapping_mul(31)).wrapping_mul(31).wrapping_add(pos.x());
    h ^ ((h as u32) >> 16) as i32
}

impl Default for JavaBlockPosHashSet {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaBlockPosHashSet {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), table: Vec::new(), tree: Vec::new(), size: 0, threshold: 0 }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Iteration order of the Java set.
    pub fn iter(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.table.iter().flat_map(move |&head| {
            let mut cur = head;
            std::iter::from_fn(move || {
                if cur == NIL {
                    return None;
                }
                let n = &self.nodes[cur];
                cur = n.next;
                Some(n.key)
            })
        })
    }

    fn new_node(&mut self, hash: i32, key: BlockPos, next: usize) -> usize {
        self.nodes.push(Node { hash, key, next, prev: NIL, parent: NIL, left: NIL, right: NIL, red: false });
        self.nodes.len() - 1
    }

    /// `HashSet.add(pos)`: whether the set changed.
    pub fn insert(&mut self, key: BlockPos) -> bool {
        let hash = spread(&key);
        if self.table.is_empty() {
            self.resize();
        }
        let n = self.table.len();
        let index = (n - 1) & hash as u32 as usize;
        let first = self.table[index];
        if first == NIL {
            let node = self.new_node(hash, key, NIL);
            self.table[index] = node;
        } else {
            if self.nodes[first].hash == hash && self.nodes[first].key == key {
                return false;
            }
            if self.tree[index] {
                if !self.put_tree_val(index, first, hash, key) {
                    return false;
                }
            } else {
                let mut p = first;
                let mut bin_count = 0;
                loop {
                    let e = self.nodes[p].next;
                    if e == NIL {
                        let node = self.new_node(hash, key, NIL);
                        self.nodes[p].next = node;
                        if bin_count >= TREEIFY_THRESHOLD - 1 {
                            self.treeify_bin(index);
                        }
                        break;
                    }
                    if self.nodes[e].hash == hash && self.nodes[e].key == key {
                        return false;
                    }
                    p = e;
                    bin_count += 1;
                }
            }
        }
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
        true
    }

    fn resize(&mut self) {
        let old_cap = self.table.len();
        let new_cap = if old_cap == 0 { 16 } else { old_cap * 2 };
        self.threshold = new_cap / 4 * 3;
        if old_cap == 0 {
            self.table = vec![NIL; new_cap];
            self.tree = vec![false; new_cap];
            return;
        }
        let old_table = std::mem::replace(&mut self.table, vec![NIL; new_cap]);
        let old_tree = std::mem::replace(&mut self.tree, vec![false; new_cap]);
        for (j, &head) in old_table.iter().enumerate() {
            if head == NIL {
                continue;
            }
            if old_tree[j] {
                self.split(head, j, old_cap);
                continue;
            }
            let (mut lo_head, mut lo_tail, mut hi_head, mut hi_tail) = (NIL, NIL, NIL, NIL);
            let mut e = head;
            while e != NIL {
                let next = self.nodes[e].next;
                if self.nodes[e].hash as u32 as usize & old_cap == 0 {
                    if lo_tail == NIL { lo_head = e } else { self.nodes[lo_tail].next = e }
                    lo_tail = e;
                } else {
                    if hi_tail == NIL { hi_head = e } else { self.nodes[hi_tail].next = e }
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

    /// `HashMap.treeifyBin(tab, hash)`.
    fn treeify_bin(&mut self, index: usize) {
        if self.table.len() < MIN_TREEIFY_CAPACITY {
            self.resize();
            return;
        }
        let head = self.table[index];
        if head == NIL {
            return;
        }
        // replacementTreeNode: prev links along the list
        let mut prev = NIL;
        let mut e = head;
        while e != NIL {
            self.nodes[e].prev = prev;
            prev = e;
            e = self.nodes[e].next;
        }
        self.tree[index] = true;
        self.treeify(head);
    }

    /// `TreeNode.treeify(tab)` of the bin starting at `head` (its slot in the current table).
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
                let h = self.nodes[x].hash;
                let mut p = root;
                loop {
                    let ph = self.nodes[p].hash;
                    let dir = if ph > h {
                        -1
                    } else if ph < h {
                        1
                    } else {
                        panic!("HashSet<BlockPos>: equal hash codes in one treeified bin (tieBreakOrder is nondeterministic in Java)");
                    };
                    let xp = p;
                    p = if dir <= 0 { self.nodes[p].left } else { self.nodes[p].right };
                    if p == NIL {
                        self.nodes[x].parent = xp;
                        if dir <= 0 { self.nodes[xp].left = x } else { self.nodes[xp].right = x }
                        root = self.balance_insertion(root, x);
                        break;
                    }
                }
            }
            x = next;
        }
        self.move_root_to_front(root);
    }

    /// `TreeNode.putTreeVal`: false when the key exists.
    fn put_tree_val(&mut self, _index: usize, first: usize, h: i32, k: BlockPos) -> bool {
        let mut root = first;
        while self.nodes[root].parent != NIL {
            root = self.nodes[root].parent;
        }
        let mut p = root;
        loop {
            let ph = self.nodes[p].hash;
            let dir = if ph > h {
                -1
            } else if ph < h {
                1
            } else if self.nodes[p].key == k {
                return false;
            } else {
                panic!("HashSet<BlockPos>: equal hash codes in one treeified bin (tieBreakOrder is nondeterministic in Java)");
            };
            let xp = p;
            p = if dir <= 0 { self.nodes[p].left } else { self.nodes[p].right };
            if p == NIL {
                let xpn = self.nodes[xp].next;
                let x = self.new_node(h, k, xpn);
                if dir <= 0 { self.nodes[xp].left = x } else { self.nodes[xp].right = x }
                self.nodes[xp].next = x;
                self.nodes[x].parent = xp;
                self.nodes[x].prev = xp;
                if xpn != NIL {
                    self.nodes[xpn].prev = x;
                }
                let new_root = self.balance_insertion(root, x);
                self.move_root_to_front(new_root);
                return true;
            }
        }
    }

    /// `TreeNode.moveRootToFront(tab, root)`.
    fn move_root_to_front(&mut self, root: usize) {
        if root == NIL || self.table.is_empty() {
            return;
        }
        let index = (self.table.len() - 1) & self.nodes[root].hash as u32 as usize;
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

    /// `TreeNode.split(map, tab, index, bit)` of the tree bin at old slot `index`.
    fn split(&mut self, b: usize, index: usize, bit: usize) {
        let (mut lo_head, mut lo_tail, mut hi_head, mut hi_tail) = (NIL, NIL, NIL, NIL);
        let (mut lc, mut hc) = (0usize, 0usize);
        let mut e = b;
        while e != NIL {
            let next = self.nodes[e].next;
            self.nodes[e].next = NIL;
            if self.nodes[e].hash as u32 as usize & bit == 0 {
                self.nodes[e].prev = lo_tail;
                if lo_tail == NIL { lo_head = e } else { self.nodes[lo_tail].next = e }
                lo_tail = e;
                lc += 1;
            } else {
                self.nodes[e].prev = hi_tail;
                if hi_tail == NIL { hi_head = e } else { self.nodes[hi_tail].next = e }
                hi_tail = e;
                hc += 1;
            }
            e = next;
        }
        if lo_head != NIL {
            if lc <= UNTREEIFY_THRESHOLD {
                self.table[index] = lo_head; // untreeify: plain list in the same order
            } else {
                self.table[index] = lo_head;
                self.tree[index] = true;
                if hi_head != NIL {
                    self.treeify(lo_head);
                }
            }
        }
        if hi_head != NIL {
            if hc <= UNTREEIFY_THRESHOLD {
                self.table[index + bit] = hi_head;
            } else {
                self.table[index + bit] = hi_head;
                self.tree[index + bit] = true;
                if lo_head != NIL {
                    self.treeify(hi_head);
                }
            }
        }
    }

    fn rotate_left(&mut self, mut root: usize, p: usize) -> usize {
        if p != NIL {
            let r = self.nodes[p].right;
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
        }
        root
    }

    fn rotate_right(&mut self, mut root: usize, p: usize) -> usize {
        if p != NIL {
            let l = self.nodes[p].left;
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
        }
        root
    }

    /// `TreeNode.balanceInsertion(root, x)`.
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
                        xp = self.nodes[x].parent;
                        xpp = if xp == NIL { NIL } else { self.nodes[xp].parent };
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
                    xp = self.nodes[x].parent;
                    xpp = if xp == NIL { NIL } else { self.nodes[xp].parent };
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
}
