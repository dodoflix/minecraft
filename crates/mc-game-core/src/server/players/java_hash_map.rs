//! `java.util.HashMap<String, V>` with Java's iteration order. Vanilla writes `ops.json`, `banned-*.json`,
//! `whitelist.json` in `HashMap.values()` order, which depends on `String.hashCode()` and the table size history
//! (grows by doubling at 0.75 load, never shrinks, `clear()` keeps the table).
//! shortcut: bins are never treeified (Java does at >= 8 colliding keys in a table of >= 64 buckets); upgrade only
//! if such pathological key sets show up.

use mc_core::math::java::java_string_hash;

struct Node<V> {
    hash: i32,
    key: String,
    value: V,
}

pub struct JavaHashMap<V> {
    table: Vec<Vec<Node<V>>>,
    size: usize,
    threshold: usize,
}

impl<V> Default for JavaHashMap<V> {
    fn default() -> Self {
        JavaHashMap {
            table: Vec::new(),
            size: 0,
            threshold: 0,
        }
    }
}

fn spread(key: &str) -> i32 {
    let h = java_string_hash(key);
    h ^ ((h as u32) >> 16) as i32
}

impl<V> JavaHashMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    fn index(&self, hash: i32) -> usize {
        (hash as u32 as usize) & (self.table.len() - 1)
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        if self.table.is_empty() {
            return None;
        }
        let hash = spread(key);
        self.table[self.index(hash)]
            .iter()
            .find(|n| n.hash == hash && n.key == key)
            .map(|n| &n.value)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// `put`: replaces in place (iteration position unchanged) or appends to the bin.
    pub fn insert(&mut self, key: String, value: V) -> Option<V> {
        if self.table.is_empty() {
            self.resize();
        }
        let hash = spread(&key);
        let idx = self.index(hash);
        if let Some(node) = self.table[idx]
            .iter_mut()
            .find(|n| n.hash == hash && n.key == key)
        {
            return Some(std::mem::replace(&mut node.value, value));
        }
        self.table[idx].push(Node { hash, key, value });
        self.size += 1;
        if self.size > self.threshold {
            self.resize();
        }
        None
    }

    pub fn remove(&mut self, key: &str) -> Option<V> {
        if self.table.is_empty() {
            return None;
        }
        let hash = spread(key);
        let idx = self.index(hash);
        let pos = self.table[idx]
            .iter()
            .position(|n| n.hash == hash && n.key == key)?;
        self.size -= 1;
        Some(self.table[idx].remove(pos).value)
    }

    /// `clear`: empties the bins but keeps the table size.
    pub fn clear(&mut self) {
        for bin in &mut self.table {
            bin.clear();
        }
        self.size = 0;
    }

    fn resize(&mut self) {
        let new_cap = if self.table.is_empty() {
            16
        } else {
            self.table.len() * 2
        };
        self.threshold = new_cap / 4 * 3;
        let old = std::mem::take(&mut self.table);
        self.table = (0..new_cap).map(|_| Vec::new()).collect();
        // bins are split into lo/hi preserving relative order, which equals re-appending in iteration order
        for node in old.into_iter().flatten() {
            let idx = (node.hash as u32 as usize) & (new_cap - 1);
            self.table[idx].push(node);
        }
    }

    /// Entries in Java iteration order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.table
            .iter()
            .flatten()
            .map(|n| (n.key.as_str(), &n.value))
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.iter().map(|(k, _)| k)
    }

    /// Remove every entry for which `pred` returns true (keeps iteration order of the rest).
    pub fn retain(&mut self, mut keep: impl FnMut(&V) -> bool) {
        for bin in &mut self.table {
            bin.retain(|n| keep(&n.value));
        }
        self.size = self.table.iter().map(Vec::len).sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden: `java.util.HashMap` keySet order of `banana, apple, cherry` and of `key0..key99` (several resizes).
    #[test]
    fn iteration_order_matches_java() {
        let mut m = JavaHashMap::new();
        for k in ["banana", "apple", "cherry"] {
            m.insert(k.to_owned(), ());
        }
        assert_eq!(m.keys().collect::<Vec<_>>(), ["banana", "apple", "cherry"]);
        let mut m = JavaHashMap::new();
        for i in 0..100 {
            m.insert(format!("key{i}"), ());
        }
        let expected = "key40 key44 key43 key42 key41 key37 key36 key35 key34 key39 key38 key51 key50 key55 key54 key53 key52 key48 key47 key46 key45 key49 key62 key61 key60 key66 key65 key64 key63 key59 key58 key57 key56 key73 key72 key71 key70 key77 key76 key75 key74 key69 key68 key67 key1 key2 key0 key5 key80 key6 key3 key4 key9 key84 key83 key7 key82 key8 key81 key88 key87 key86 key85 key79 key78 key91 key90 key95 key94 key93 key92 key11 key99 key10 key98 key97 key96 key89 key22 key21 key20 key15 key14 key13 key12 key19 key18 key17 key16 key33 key32 key31 key30 key26 key25 key24 key23 key29 key28 key27";
        assert_eq!(m.keys().collect::<Vec<_>>().join(" "), expected);
    }

    #[test]
    fn resize_keeps_everything() {
        let mut m = JavaHashMap::new();
        for i in 0..100 {
            m.insert(format!("key{i}"), i);
        }
        assert_eq!(m.len(), 100);
        for i in 0..100 {
            assert_eq!(m.get(&format!("key{i}")), Some(&i));
        }
        m.remove("key5");
        assert_eq!(m.len(), 99);
        m.clear();
        assert!(m.is_empty());
        m.insert("x".into(), 1);
        assert_eq!(m.get("x"), Some(&1));
    }
}
