//! The file fixer on real folders: the copy-on-write view, the swap of the new world, and what an error or a killed
//! process leaves behind. A killed process is a real one: the test runs itself again with an environment variable
//! and the child aborts at the step it was told to.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex, Once};

use mc_core::pos::chunk_pos::ChunkPos;
use mc_datafix::{
    DSL, DataFix, DataFixTypes, DataFixer, DataFixerBuilder, References, Schema, SchemaDef,
    TemplateMap, TypeRewriteRule,
};
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_nbt::{CompoundTag, NbtAccounter, Tag};

use super::*;
use crate::level_storage::{LevelStorageSource, read_compressed, write_compressed};
use crate::region::{RegionFileStorage, RegionStorageInfo};

// ---- log capture -------------------------------------------------------------------------------------------------

thread_local! {
    static LOGGED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGGED.with(|l| {
            l.borrow_mut()
                .push(format!("{}: {}", record.level(), record.args()))
        });
    }
    fn flush(&self) {}
}

fn capture_logs<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::set_logger(&CaptureLogger).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Trace);
    });
    LOGGED.with(|l| l.borrow_mut().clear());
    let result = f();
    (
        result,
        LOGGED.with(|l| std::mem::take(&mut *l.borrow_mut())),
    )
}

// ---- a data fixer that passes level data through ---------------------------------------------------------------------

struct LevelOnly;

impl SchemaDef for LevelOnly {
    fn register_entities(&self, _this: &Schema, _schema: &Schema) -> TemplateMap {
        TemplateMap::new()
    }
    fn register_block_entities(&self, _this: &Schema, _schema: &Schema) -> TemplateMap {
        TemplateMap::new()
    }
    fn register_types(&self, _this: &Schema, schema: &Schema, _e: &TemplateMap, _b: &TemplateMap) {
        schema.register_type(false, &References::LEVEL, |_| DSL::remainder());
        // `Schema::same` needs a recursive type in the schema it continues.
        schema.register_type(false, &References::ITEM_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(true, &References::ITEM_STACK, |s| {
            DSL::optional_fields(&[("id", References::ITEM_NAME.in_(s))])
        });
    }
}

fn test_data_fixer() -> &'static DataFixer {
    static FIXER: LazyLock<DataFixer> = LazyLock::new(|| {
        let mut b = DataFixerBuilder::new(mc_datafix::CURRENT_VERSION);
        b.add_schema(99, |k, p| Schema::new(k, p, LevelOnly));
        let v100 = b.add_schema(100, Schema::same);
        b.add_fixer(DataFix::new("Nop", &v100, false, |_| {
            TypeRewriteRule::nop()
        }));
        b.build()
    });
    &FIXER
}

// ---- the world and its fixes -----------------------------------------------------------------------------------------

const WORLD_VERSION: i32 = 4772;
const FIX_VERSION: i32 = 4773;
const LATEST: i32 = 4899;

fn level_dat(data_version: i32) -> CompoundTag {
    let mut data = CompoundTag::new();
    data.put_int("DataVersion", data_version);
    data.put_string("LevelName", "world");
    data.put_int("version", 19133);
    let mut root = CompoundTag::new();
    root.put("Data", data);
    root
}

fn saved_data(data_version: i32, counter: i32) -> CompoundTag {
    let mut data = CompoundTag::new();
    data.put_int("counter", counter);
    let mut root = CompoundTag::new();
    root.put("data", data);
    root.put_int("DataVersion", data_version);
    root
}

fn chunk_pos() -> ChunkPos {
    ChunkPos::new(1, 2)
}

fn region_info() -> RegionStorageInfo {
    RegionStorageInfo::new(
        "world",
        mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(),
        "chunk",
    )
}

/// Writes the world `<saves>/world`.
fn write_world(saves: &Path) -> PathBuf {
    let world = saves.join("world");
    for dir in ["playerdata", "stats", "region", "data", "empty"] {
        fs::create_dir_all(world.join(dir)).unwrap();
    }
    write_compressed(&level_dat(WORLD_VERSION), &world.join("level.dat")).unwrap();
    write_compressed(&level_dat(WORLD_VERSION), &world.join("level.dat_old")).unwrap();
    fs::write(world.join("session.lock"), "☃").unwrap();
    fs::write(world.join("playerdata/p.dat"), b"player").unwrap();
    fs::write(world.join("stats/s.json"), b"{}").unwrap();
    fs::write(world.join("resources.zip"), b"zip").unwrap();
    write_compressed(&saved_data(FIX_VERSION, 1), &world.join("data/counter.dat")).unwrap();
    let mut storage = RegionFileStorage::new(region_info(), world.join("region"), false);
    let mut chunk = CompoundTag::new();
    chunk.put_int("DataVersion", mc_datafix::CURRENT_VERSION);
    chunk.put_int("foo", 1);
    storage.write(&chunk_pos(), Some(&chunk)).unwrap();
    storage.close().unwrap();
    world
}

/// The fix of [`FIX_VERSION`]: moves a folder and a file, deletes an empty folder, rewrites a saved data file and a
/// chunk.
fn fix_of_version() -> FileFix {
    let mut fix = FileFix::new(FIX_VERSION);
    fix.add_file_fix_operation(FileFixOperation::Move(Move::new(
        "playerdata",
        "players/data",
    )));
    fix.add_file_fix_operation(FileFixOperation::Move(Move::new(
        "resources.zip",
        "resourcepacks/resources.zip",
    )));
    fix.add_file_fix_operation(FileFixOperation::delete("empty"));
    fix.add_file_content_fix(|ctx| {
        let counter = only_file(ctx.saved_data(
            References::SAVED_DATA_GAME_RULES,
            &FileRelation::data().for_file("counter.dat"),
            MissingSeverity::Neutral,
        ))?;
        let data = counter.read(ctx.tree)?.expect("counter.dat exists");
        let next = data.get("counter").as_int_or(0) + 1;
        counter.write(ctx.tree, &data.set("counter", data.create_int(next)))?;
        let mut chunks = only_file(ctx.chunk_nbt(
            &FileRelation::region(),
            &region_info(),
            DataFixTypes::CHUNK,
        )?)?;
        chunks.update_chunk(ctx.tree, &chunk_pos(), None, |mut tag| {
            tag.put_int("foo", 2);
            tag
        })?;
        // A chunk that is not stored stays absent.
        chunks.update_chunk(ctx.tree, &ChunkPos::new(7, 7), None, |tag| tag)?;
        Ok(chunks.close(ctx.tree)?)
    });
    fix
}

fn fixer(hooks: Option<TestHooks>) -> FileFixerUpper {
    let fixer = FileFixerUpper::new(test_data_fixer(), vec![fix_of_version()], LATEST);
    match hooks {
        Some(hooks) => fixer.with_hooks(Box::new(hooks)),
        None => fixer,
    }
}

#[derive(Clone, Default)]
struct TestHooks {
    capabilities: Option<FileSystemCapabilities>,
    fail_at: Option<Checkpoint>,
    abort_at: Option<Checkpoint>,
    /// Runs at every checkpoint.
    action: Option<Arc<dyn Fn(Checkpoint) + Send + Sync>>,
}

const NO_HARD_LINKS: FileSystemCapabilities = FileSystemCapabilities {
    atomic_move: true,
    hard_links: false,
};
const HARD_LINKS: FileSystemCapabilities = FileSystemCapabilities {
    atomic_move: true,
    hard_links: true,
};

impl FixHooks for TestHooks {
    fn file_system_capabilities(&self, dir: &Path) -> io::Result<FileSystemCapabilities> {
        match self.capabilities {
            Some(capabilities) => Ok(capabilities),
            None => detect_file_system_capabilities(dir),
        }
    }

    fn checkpoint(&self, at: Checkpoint) -> io::Result<()> {
        if let Some(action) = &self.action {
            action(at);
        }
        if self.abort_at == Some(at) {
            std::process::abort();
        }
        if self.fail_at == Some(at) {
            return Err(io::Error::other(format!("injected failure at {at:?}")));
        }
        Ok(())
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl UpgradeNotifications for Recorder {
    fn world_upgrade_started(&self) {
        self.0.lock().unwrap().push("started".into());
    }
    fn world_upgrade_finished(&self) {
        self.0.lock().unwrap().push("finished".into());
    }
    fn world_upgrade_failed(&self, reason: &str) {
        self.0.lock().unwrap().push(format!("failed: {reason}"));
    }
}

fn progress() -> (UpgradeProgress, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    (UpgradeProgress::new(recorder.clone()), recorder)
}

/// Every file (bytes) and directory (`None`) below `dir`, by relative path.
fn snapshot(dir: &Path, skip: &[&str]) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(root: &Path, dir: &Path, skip: &[&str], out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if skip.contains(&relative.as_str()) {
                continue;
            }
            if path.is_dir() {
                out.insert(relative, None);
                walk(root, &path, skip, out);
            } else {
                out.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, skip, &mut out);
    out
}

/// The paths of a snapshot with the content of the files that a fix rewrites left out.
fn layout(snapshot: &BTreeMap<String, Option<Vec<u8>>>) -> Vec<&str> {
    snapshot.keys().map(String::as_str).collect()
}

/// The layout of the world after the fix of [`FIX_VERSION`].
fn fixed_layout() -> Vec<&'static str> {
    vec![
        "data",
        "data/counter.dat",
        "level.dat",
        "players",
        "players/data",
        "players/data/p.dat",
        "region",
        "region/r.0.0.mca",
        "resourcepacks",
        "resourcepacks/resources.zip",
        "session.lock",
        "stats",
        "stats/s.json",
    ]
}

/// The names in the saves folder.
fn saves_entries(saves: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(saves)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

struct Setup {
    _tmp: Option<tempfile::TempDir>,
    saves: PathBuf,
    world: PathBuf,
}

fn setup() -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let saves = tmp.path().join("saves");
    fs::create_dir_all(&saves).unwrap();
    let world = write_world(&saves);
    Setup {
        _tmp: Some(tmp),
        saves,
        world,
    }
}

fn run(
    setup: &Setup,
    fixer: &FileFixerUpper,
    progress: &UpgradeProgress,
) -> Result<CompoundTag, FileFixError> {
    let source = LevelStorageSource::create_default(&setup.saves).unwrap();
    let mut access = source.create_access("world").unwrap();
    let tag = access.get_unfixed_data_tag(false).unwrap();
    fixer.fix_to(&mut access, tag, progress, LATEST)
}

fn assert_fixed_world(world: &Path) {
    let snap = snapshot(world, &[]);
    assert_eq!(layout(&snap), fixed_layout());
    let counter = read_compressed(
        &world.join("data/counter.dat"),
        &mut NbtAccounter::unlimited_heap(),
    )
    .unwrap();
    assert_eq!(
        counter
            .get_compound_or_empty("data")
            .get_int_or("counter", 0),
        2
    );
    assert_eq!(counter.get_int_or("DataVersion", 0), FIX_VERSION);
    assert_eq!(snap["players/data/p.dat"].as_deref(), Some(&b"player"[..]));
    let mut storage = RegionFileStorage::new(region_info(), world.join("region"), false);
    let chunk = storage
        .read(&chunk_pos())
        .unwrap()
        .expect("the chunk is stored");
    assert_eq!(chunk.get_int_or("foo", 0), 2);
    assert!(storage.read(&ChunkPos::new(7, 7)).unwrap().is_none());
    let level = read_compressed(
        &world.join("level.dat"),
        &mut NbtAccounter::unlimited_heap(),
    )
    .unwrap();
    let data = level.get_compound_or_empty("Data");
    assert_eq!(data.get_int_or("DataVersion", 0), LATEST);
    assert_eq!(
        data.get_compound_or_empty("Version").get_int_or("Id", 0),
        mc_datafix::CURRENT_VERSION
    );
    assert!(data.get_long_or("LastPlayed", 0) > 0);
}

// ---- the copy-on-write view --------------------------------------------------------------------------------------------

/// The `IOException` of a failed operation.
fn io_error(e: FileFixError) -> io::Error {
    match e {
        FileFixError::Io(e) => e,
        other => panic!("not an I/O error: {other:?}"),
    }
}

fn cow_message(e: &io::Error) -> String {
    let exception = mc_core::java_exception::JavaException::of_io(e);
    match exception.class.contains("CowFS") {
        true => exception.message.unwrap_or_default(),
        false => format!("not a CowFsError: {e}"),
    }
}

#[test]
fn the_view_refuses_a_symlink_a_missing_write_access_and_an_existing_temporary_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let world = tmp.path().join("world");
    fs::create_dir_all(world.join("sub")).unwrap();
    fs::write(world.join("a.dat"), b"a").unwrap();
    let tmp_dir = tmp.path().join("cow");

    fs::create_dir(&tmp_dir).unwrap();
    let e = CowTree::create(&world, &tmp_dir, |_| false).err().unwrap();
    assert_eq!(
        cow_message(&e),
        format!("Temporary directory already exists: {}", tmp_dir.display())
    );
    fs::remove_dir(&tmp_dir).unwrap();

    #[cfg(unix)]
    {
        let link = world.join("sub/link");
        std::os::unix::fs::symlink(world.join("a.dat"), &link).unwrap();
        let e = CowTree::create(&world, &tmp_dir, |_| false).err().unwrap();
        assert_eq!(
            cow_message(&e),
            format!(
                "Cannot build copy-on-write file system when symlink is present: {}",
                link.display()
            )
        );
        assert!(!tmp_dir.exists(), "a refused view leaves nothing behind");
        fs::remove_file(&link).unwrap();

        // `access(W_OK)` lets root write anything.
        // SAFETY: `geteuid` has no preconditions.
        if unsafe { libc::geteuid() } != 0 {
            use std::os::unix::fs::PermissionsExt;
            let locked = world.join("sub/locked.dat");
            fs::write(&locked, b"x").unwrap();
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o444)).unwrap();
            let e = CowTree::create(&world, &tmp_dir, |_| false).err().unwrap();
            assert_eq!(
                cow_message(&e),
                format!(
                    "Cannot build copy-on-write file system, missing write access for file: {}",
                    locked.display()
                )
            );
        }
    }
}

#[test]
fn the_view_copies_a_file_on_its_first_write_and_leaves_the_world_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let world = tmp.path().join("world");
    fs::create_dir_all(world.join("d/e")).unwrap();
    fs::write(world.join("d/a.dat"), b"original").unwrap();
    fs::write(world.join("d/e/b.dat"), b"b").unwrap();
    let before = snapshot(&world, &[]);
    let mut tree = CowTree::create(&world, &tmp.path().join("cow"), |_| false).unwrap();

    // `HashMap` order: `e` is in bucket 5 and `a.dat` in bucket 7 of the 16
    assert_eq!(tree.list(Path::new("d")).unwrap(), ["e", "a.dat"]);
    assert!(tree.is_dir(Path::new("d/e")) && tree.is_file(Path::new("d/a.dat")));
    assert_eq!(
        tree.file_for_read(Path::new("d/a.dat")).unwrap(),
        world.join("d/a.dat")
    );

    let copy = tree.file_for_write(Path::new("d/a.dat"), false).unwrap();
    assert_eq!(copy, tmp.path().join("cow/tmp_1"));
    fs::write(&copy, b"changed").unwrap();
    assert_eq!(
        tree.file_for_write(Path::new("d/a.dat"), false).unwrap(),
        copy,
        "one copy"
    );
    assert_eq!(fs::read(world.join("d/a.dat")).unwrap(), b"original");

    // A directory moves with its content; `..` stops at the root.
    tree.create_dir_all(Path::new("x/y")).unwrap();
    tree.rename(Path::new("d/e"), Path::new("x/y/e")).unwrap();
    assert!(!tree.exists(Path::new("d/e")) && tree.is_file(Path::new("x/y/e/b.dat")));
    assert!(tree.exists(Path::new("../../x/./y")));
    let e = tree.rename(Path::new("x"), Path::new("x/y/z")).unwrap_err();
    assert!(
        cow_message(&e).ends_with("can't move a directory into itself"),
        "{e}"
    );
    let e = tree
        .rename(Path::new("d/a.dat"), Path::new("x/y"))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
    let e = tree
        .rename(Path::new("nothing"), Path::new("n"))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotFound);
    let e = tree.delete(Path::new("x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::DirectoryNotEmpty);
    assert_eq!(
        cow_message(&tree.delete(Path::new("")).unwrap_err()),
        "Can't remove root"
    );
    assert!(tree.delete_if_exists(Path::new("d/a.dat")).unwrap());
    assert!(!copy.exists(), "deleting a file deletes its copy");
    assert!(!tree.delete_if_exists(Path::new("d/a.dat")).unwrap());

    let moves = tree.collect_move_operations(Path::new("/out"));
    assert_eq!(
        moves.directories,
        [
            Path::new("/out/d"),
            Path::new("/out/x"),
            Path::new("/out/x/y"),
            Path::new("/out/x/y/e")
        ]
    );
    assert!(moves.copied_files.is_empty());
    assert_eq!(
        moves.preexisting_files,
        [FileMove {
            from: world.join("d/e/b.dat"),
            to: "/out/x/y/e/b.dat".into()
        }]
    );
    assert_eq!(
        snapshot(&world, &[]),
        before,
        "the view never touches the world"
    );
    tree.close().unwrap();
    assert!(!tmp.path().join("cow").exists());
}

#[test]
fn operations_skip_a_missing_source_and_refuse_an_existing_target() {
    let tmp = tempfile::tempdir().unwrap();
    let world = tmp.path().join("world");
    fs::create_dir_all(world.join("data")).unwrap();
    fs::write(world.join("data/command_storage_ns.dat"), b"1").unwrap();
    fs::write(world.join("data/map_12.dat"), b"2").unwrap();
    fs::write(world.join("data/map_x.dat"), b"3").unwrap();
    fs::write(world.join("a"), b"a").unwrap();
    fs::write(world.join("b"), b"b").unwrap();
    let mut tree = CowTree::create(&world, &tmp.path().join("cow"), |_| false).unwrap();
    let progress = UpgradeProgress::default();

    let base = Path::new("");
    FileFixOperation::Move(Move::new("missing", "elsewhere"))
        .fix(&mut tree, base, &progress)
        .unwrap();
    assert!(!tree.exists(Path::new("elsewhere")));
    let e = io_error(
        FileFixOperation::Move(Move::new("a", "b"))
            .fix(&mut tree, base, &progress)
            .unwrap_err(),
    );
    assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        e.to_string(),
        "java.nio.file.FileAlreadyExistsException: Cannot move file from a to b: Target already exists"
    );

    FileFixOperation::ApplyInFolders {
        folders: FileRelation::data(),
        operations: vec![
            FileFixOperation::RegexMove(RegexMove::new(
                "command_storage_([a-z0-9_.-]+)\\.dat",
                "$1/command_storage\\.dat",
            )),
            FileFixOperation::RegexMove(RegexMove::new(
                "map_(\\d+)\\.dat",
                "minecraft/maps/$1\\.dat",
            )),
        ],
    }
    .fix(&mut tree, base, &progress)
    .unwrap();
    let mut listed = tree.list(Path::new("data")).unwrap();
    listed.sort();
    assert_eq!(listed, ["map_x.dat", "minecraft", "ns"]);
    assert!(tree.is_file(Path::new("data/ns/command_storage.dat")));
    assert!(tree.is_file(Path::new("data/minecraft/maps/12.dat")));

    // A group move, as the dimension storage fix makes it: the empty source folder gives `/region`, the root's.
    tree.create_dir_all(Path::new("region")).unwrap();
    tree.create_dir_all(Path::new("DIM1/region")).unwrap();
    FileFixOperation::GroupMove {
        from_to: vec![
            (String::new(), "dimensions/minecraft/overworld".to_owned()),
            ("DIM1".to_owned(), "dimensions/minecraft/the_end".to_owned()),
        ],
        moves: vec![Move::simple("region"), Move::simple("poi")],
    }
    .fix(&mut tree, base, &progress)
    .unwrap();
    assert!(!tree.exists(Path::new("region")) && !tree.exists(Path::new("DIM1/region")));
    assert!(tree.is_dir(Path::new("dimensions/minecraft/overworld/region")));
    assert!(tree.is_dir(Path::new("dimensions/minecraft/the_end/region")));

    // Deleting: a folder with content stays (warning), an empty one and a lone `.DS_Store` folder go.
    tree.create_dir_all(Path::new("only_ds")).unwrap();
    tree.file_for_write(Path::new("only_ds/.DS_Store"), true)
        .unwrap();
    tree.create_dir_all(Path::new("empty")).unwrap();
    let ((), logs) = capture_logs(|| {
        for target in ["data", "only_ds", "empty", "missing"] {
            FileFixOperation::delete(target)
                .fix(&mut tree, base, &progress)
                .unwrap();
        }
    });
    assert!(tree.is_dir(Path::new("data")));
    assert!(!tree.exists(Path::new("only_ds")) && !tree.exists(Path::new("empty")));
    assert_eq!(
        logs.iter()
            .filter(|l| l.starts_with(
                "WARN: Failed to delete directory '/data', as it's not empty. Content: ["
            ))
            .count(),
        1,
        "{logs:?}"
    );
}

#[test]
fn relations_find_the_dimensions_of_a_world() {
    let tmp = tempfile::tempdir().unwrap();
    let world = tmp.path().join("world");
    fs::create_dir_all(&world).unwrap();
    let mut tree = CowTree::create(&world, &tmp.path().join("cow"), |_| false).unwrap();
    let dimensions = |tree: &CowTree| {
        FileRelation::dimensions()
            .paths(tree, Path::new(""))
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        dimensions(&tree),
        [
            "dimensions/minecraft/overworld",
            "dimensions/minecraft/the_nether",
            "dimensions/minecraft/the_end"
        ]
    );
    tree.create_dir_all(Path::new("dimensions/mod/mine"))
        .unwrap();
    tree.create_dir_all(Path::new("generated/ns/structures"))
        .unwrap();
    assert_eq!(dimensions(&tree), ["dimensions/mod/mine"]);
    assert_eq!(
        FileRelation::generated_namespaces().paths(&tree, Path::new("")),
        [PathBuf::from("generated/ns")]
    );
    let joined = FileRelation::join(&[
        FileRelation::origin(),
        FileRelation::origin(),
        FileRelation::data(),
    ]);
    assert_eq!(
        joined.paths(&tree, Path::new("x")),
        [PathBuf::from("x"), PathBuf::from("x/data")]
    );
}

// ---- the upgrade --------------------------------------------------------------------------------------------------------

#[test]
fn a_world_that_does_not_need_file_fixing_is_only_fixed_in_memory() {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let (progress, recorder) = progress();
    let fixer = FileFixerUpper::new(test_data_fixer(), vec![fix_of_version()], WORLD_VERSION);
    let (result, logs) = capture_logs(|| run(&setup, &fixer, &progress));
    let tag = result.unwrap();
    assert_eq!(tag.get_int_or("DataVersion", 0), LATEST);
    assert_eq!(snapshot(&setup.world, &[]), before);
    assert!(logs.is_empty(), "{logs:?}");
    assert!(recorder.0.lock().unwrap().is_empty());
}

#[test]
fn the_new_world_replaces_the_old_one_with_hard_links() {
    let setup = setup();
    let (progress, recorder) = progress();
    let fixer = fixer(Some(TestHooks {
        capabilities: Some(HARD_LINKS),
        ..TestHooks::default()
    }));
    let (result, logs) = capture_logs(|| run(&setup, &fixer, &progress));
    let tag = result.unwrap();
    assert_eq!(tag.get_int_or("DataVersion", 0), LATEST);
    assert_fixed_world(&setup.world);
    assert_eq!(saves_entries(&setup.saves), ["world"]);
    assert_eq!(*recorder.0.lock().unwrap(), ["started", "finished"]);
    let expected = [
        "INFO: Starting upgrade for world \"world\"",
        "INFO: File system capabilities: FileSystemCapabilities[atomicMove=true, hardLinks=true]",
        "INFO: Applying file structure changes for world \"world\"",
        "INFO: Moving new hardlinked world to top level",
        "INFO: Start cleanup",
        "INFO: Moving out old world folder",
        "INFO: Moving in new world folder",
        "INFO: Done applying file structure changes for world \"world\". Cleaning up outdated data...",
        "INFO: Upgrade done for world \"world\"",
    ];
    let relevant: Vec<&String> = logs
        .iter()
        .filter(|l| !l.contains("Upgrading progress"))
        .collect();
    assert_eq!(relevant, expected);
}

/// The log lines of the upgrade without the time dependent progress lines.
fn without_progress(logs: &[String]) -> Vec<&String> {
    logs.iter()
        .filter(|l| !l.contains("Upgrading progress"))
        .collect()
}

fn inode(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).unwrap().ino()
}

#[test]
fn the_moves_of_a_file_system_without_hard_links_are_journaled_and_done() {
    let setup = setup();
    let inode_before = inode(&setup.world.join("stats/s.json"));
    let (progress, recorder) = progress();
    let fixer = fixer(Some(TestHooks {
        capabilities: Some(NO_HARD_LINKS),
        ..TestHooks::default()
    }));
    let (result, logs) = capture_logs(|| run(&setup, &fixer, &progress));
    result.unwrap();
    assert_fixed_world(&setup.world);
    assert_eq!(saves_entries(&setup.saves), ["world"]);
    assert_eq!(
        inode(&setup.world.join("stats/s.json")),
        inode_before,
        "an untouched file is moved, not copied"
    );
    assert_eq!(*recorder.0.lock().unwrap(), ["started", "finished"]);
    let expected = [
        "INFO: Starting upgrade for world \"world\"",
        "INFO: File system capabilities: FileSystemCapabilities[atomicMove=true, hardLinks=false]",
        "INFO: Applying file structure changes for world \"world\"",
        "INFO: Moving files into new file structure",
        "INFO: Moving new world to top level",
        "INFO: Complete move",
        "INFO: Start cleanup",
        "INFO: Moving out old world folder",
        "INFO: Moving in new world folder",
        "INFO: Done applying file structure changes for world \"world\". Cleaning up outdated data...",
        "INFO: Upgrade done for world \"world\"",
    ];
    assert_eq!(without_progress(&logs), expected);
    assert_eq!(
        logs[1], "INFO: Upgrading progress: 25%",
        "the first operation finished: 1 of 4"
    );
    assert_eq!(progress.type_(), Some(Type::Files));
    assert_eq!(progress.status(), Status::Upgrading);
    assert_eq!(
        (
            progress.total_file_fix_stats().finished_operations(),
            progress.total_file_fix_stats().total_operations()
        ),
        (4, 4)
    );
    assert_eq!(progress.running_file_fixer_stats().finished_operations(), 1);
}

/// What an upgrade that fails before the swap leaves: the world as it was, nothing else.
fn assert_world_untouched(setup: &Setup, before: &BTreeMap<String, Option<Vec<u8>>>) {
    assert_eq!(&snapshot(&setup.world, &[]), before);
    assert_eq!(saves_entries(&setup.saves), ["world"]);
}

#[test]
fn a_fix_that_fails_after_changing_the_view_leaves_the_world_untouched() {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let mut failing = fix_of_version();
    failing.add_file_content_fix(|_| Err(io::Error::other("the fix failed").into()));
    let fixer = FileFixerUpper::new(test_data_fixer(), vec![failing], LATEST);
    let (progress, recorder) = progress();
    let error = run(&setup, &fixer, &progress).unwrap_err();
    let FileFixError::Aborted {
        cause,
        not_reverted_moves,
        capabilities,
    } = &error
    else {
        panic!("{error:?}");
    };
    assert_eq!(cause.to_string(), "the fix failed");
    assert!(not_reverted_moves.is_empty() && capabilities.is_none());
    assert_world_untouched(&setup, &before);
    assert_eq!(
        *recorder.0.lock().unwrap(),
        [
            "started",
            "failed: The world upgrade failed during file fixing"
        ]
    );
}

#[test]
fn a_canceled_upgrade_leaves_the_world_untouched() {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let (progress, recorder) = progress();
    progress.set_canceled();
    let error = run(&setup, &fixer(None), &progress).unwrap_err();
    assert!(matches!(error, FileFixError::Canceled), "{error:?}");
    assert_world_untouched(&setup, &before);
    assert_eq!(*recorder.0.lock().unwrap(), ["started"]);
}

#[test]
fn a_failure_after_the_fixes_ran_leaves_the_world_untouched() {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let hooks = TestHooks {
        fail_at: Some(Checkpoint::FixesApplied),
        ..TestHooks::default()
    };
    let (progress, recorder) = progress();
    let error = run(&setup, &fixer(Some(hooks)), &progress).unwrap_err();
    assert!(matches!(error, FileFixError::Aborted { .. }), "{error:?}");
    assert_world_untouched(&setup, &before);
    assert_eq!(
        *recorder.0.lock().unwrap(),
        [
            "started",
            "failed: The world upgrade failed during file fixing"
        ]
    );
}

#[test]
fn a_failing_swap_without_hard_links_moves_every_file_back() {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let hooks = TestHooks {
        capabilities: Some(NO_HARD_LINKS),
        fail_at: Some(Checkpoint::MoveFile(2)),
        ..TestHooks::default()
    };
    let (progress, recorder) = progress();
    let (result, logs) = capture_logs(|| run(&setup, &fixer(Some(hooks)), &progress));
    let error = result.unwrap_err();
    let FileFixError::Aborted {
        not_reverted_moves,
        capabilities,
        ..
    } = &error
    else {
        panic!("{error:?}");
    };
    assert!(not_reverted_moves.is_empty());
    assert_eq!(*capabilities, Some(NO_HARD_LINKS));
    assert_world_untouched(&setup, &before);
    assert_eq!(
        *recorder.0.lock().unwrap(),
        [
            "started",
            "failed: The world upgrade failed while swapping in the file-fixed world"
        ]
    );
    let lines = without_progress(&logs);
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.starts_with("INFO: Reverted move from "))
            .count(),
        2,
        "{lines:?}"
    );
    assert!(
        lines.contains(&&"INFO: Successfully reverted back to previous world state".to_owned())
    );
    assert!(lines.iter().any(|l| l.starts_with(
        "ERROR: Encountered error while trying to create new world folder: injected failure"
    )));
}

#[test]
fn moves_that_cannot_be_reverted_are_reported_and_the_journal_stays() {
    let setup = setup();
    let new_world = setup.world.join("filefix/new_world");
    let sabotage = Arc::new(move |at: Checkpoint| {
        // A file already moved is gone from where it was moved to: it cannot be moved back.
        if at == Checkpoint::MoveFile(2) {
            fs::remove_file(new_world.join("players/data/p.dat")).unwrap();
        }
    });
    let hooks = TestHooks {
        capabilities: Some(NO_HARD_LINKS),
        fail_at: Some(Checkpoint::MoveFile(2)),
        action: Some(sabotage),
        ..TestHooks::default()
    };
    let (progress, recorder) = progress();
    let error = run(&setup, &fixer(Some(hooks)), &progress).unwrap_err();
    let FileFixError::Aborted {
        not_reverted_moves, ..
    } = &error
    else {
        panic!("{error:?}");
    };
    assert_eq!(not_reverted_moves.len(), 1);
    assert!(not_reverted_moves[0].to.ends_with("players/data/p.dat"));
    assert!(
        setup
            .world
            .join("filefix/upgrade_in_progress.json")
            .is_file(),
        "the journal is kept for the next start"
    );
    assert_eq!(
        *recorder.0.lock().unwrap(),
        [
            "started",
            "failed: The world upgrade failed while swapping in the file-fixed world and some moves were not reverted"
        ]
    );
    let report = error.crash_report().unwrap();
    assert_eq!(report.title, "Upgrading world failed with errors");
    assert_eq!(report.categories.len(), 2);
    assert_eq!(
        report.categories[0],
        CrashReportCategory {
            name: "File system capabilities".into(),
            details: vec![
                ("Hard Links".into(), "false".into()),
                ("Atomic Move".into(), "true".into())
            ],
        }
    );
    assert_eq!(report.categories[1].name, "Moves that failed to revert");
    assert_eq!(report.categories[1].details[0].0, "0");
    assert!(
        report.categories[1].details[0]
            .1
            .ends_with("players/data/p.dat")
    );
}

#[test]
fn a_file_system_without_atomic_moves_is_refused_and_the_world_is_untouched() {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let hooks = TestHooks {
        capabilities: Some(FileSystemCapabilities {
            atomic_move: false,
            hard_links: true,
        }),
        ..TestHooks::default()
    };
    let error = run(&setup, &fixer(Some(hooks)), &UpgradeProgress::default()).unwrap_err();
    assert!(
        matches!(error, FileFixError::AtomicMoveNotSupported { .. }),
        "{error:?}"
    );
    // The leftover `filefix` folder is deleted by the next start.
    let mut after = snapshot(&setup.world, &["filefix"]);
    after.retain(|k, _| !k.starts_with("filefix/"));
    assert_eq!(after, before);
    let report = error.crash_report().unwrap();
    assert_eq!(report.categories.len(), 1);
    assert_eq!(
        report.categories[0].details[1],
        ("Atomic Move".into(), "false".into())
    );
    assert_eq!(saves_entries(&setup.saves), ["world"]);
}

#[test]
fn a_failing_move_of_the_hard_linked_world_leaves_the_world_untouched_and_the_upgrade_can_be_repeated()
 {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let hooks = TestHooks {
        capabilities: Some(HARD_LINKS),
        fail_at: Some(Checkpoint::NewWorldAtTopLevel),
        ..TestHooks::default()
    };
    let error = run(&setup, &fixer(Some(hooks)), &UpgradeProgress::default()).unwrap_err();
    assert!(matches!(error, FileFixError::Aborted { .. }), "{error:?}");
    assert_eq!(snapshot(&setup.world, &[]), before);
    // The new world was already beside the old one: it stays as a folder of its own.
    assert_eq!(saves_entries(&setup.saves), ["world", "world upgraded"]);

    let again = fixer(Some(TestHooks {
        capabilities: Some(HARD_LINKS),
        ..TestHooks::default()
    }));
    run(&setup, &again, &UpgradeProgress::default()).unwrap();
    assert_fixed_world(&setup.world);
    assert_eq!(saves_entries(&setup.saves), ["world", "world upgraded"]);
}

#[test]
fn a_new_world_that_cannot_be_moved_in_is_reported_with_its_folder() {
    let setup = setup();
    // The injected failure comes after the move: it stands for a move that fails, only the report is looked at.
    let hooks = TestHooks {
        capabilities: Some(HARD_LINKS),
        fail_at: Some(Checkpoint::NewWorldMovedIn),
        ..TestHooks::default()
    };
    let (progress, recorder) = progress();
    let error = run(&setup, &fixer(Some(hooks)), &progress).unwrap_err();
    let FileFixError::FailedCleanup {
        new_world_folder_name,
        ..
    } = &error
    else {
        panic!("{error:?}");
    };
    assert_eq!(new_world_folder_name, "world upgraded");
    assert_eq!(*recorder.0.lock().unwrap(), ["started"]);
    let report = error.crash_report().unwrap();
    assert_eq!(
        report.categories[1],
        CrashReportCategory {
            name: "World upgrade".into(),
            details: vec![("New Name".into(), "world upgraded".into())],
        }
    );
}

// ---- a process that is killed ------------------------------------------------------------------------------------------

const CHILD_ENV: &str = "MC_FILEFIX_TEST_CHILD";

fn parse_checkpoint(text: &str) -> Checkpoint {
    match text.split_once(':') {
        Some(("MoveFile", index)) => Checkpoint::MoveFile(index.parse().unwrap()),
        None => match text {
            "FixesApplied" => Checkpoint::FixesApplied,
            "JournalWritten" => Checkpoint::JournalWritten,
            "NewWorldAtTopLevel" => Checkpoint::NewWorldAtTopLevel,
            "OldWorldMovedOut" => Checkpoint::OldWorldMovedOut,
            other => panic!("unknown checkpoint {other}"),
        },
        Some(_) => panic!("unknown checkpoint {text}"),
    }
}

/// The body of the child process of [`kill_at`]; a normal run of the test does nothing here.
#[test]
fn child_process_entry() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let parts: Vec<&str> = spec.split('|').collect();
    let setup = Setup {
        _tmp: None,
        saves: PathBuf::from(parts[0]),
        world: PathBuf::from(parts[0]).join("world"),
    };
    let hooks = TestHooks {
        capabilities: Some(if parts[1] == "hard_links" {
            HARD_LINKS
        } else {
            NO_HARD_LINKS
        }),
        abort_at: Some(parse_checkpoint(parts[2])),
        ..TestHooks::default()
    };
    let _ = run(&setup, &fixer(Some(hooks)), &UpgradeProgress::default());
}

/// Runs the upgrade of the world in `saves` in a process of its own that aborts at `checkpoint`.
fn kill_at(saves: &Path, hard_links: bool, checkpoint: &str) {
    use std::os::unix::process::ExitStatusExt;
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "filefix::tests::child_process_entry",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            CHILD_ENV,
            format!(
                "{}|{}|{checkpoint}",
                saves.display(),
                if hard_links { "hard_links" } else { "journal" }
            ),
        )
        .status()
        .unwrap();
    assert_eq!(
        status.signal(),
        Some(6),
        "the child must abort at {checkpoint}, got {status:?}"
    );
}

#[test]
fn a_kill_before_the_swap_leaves_the_world_untouched_and_the_upgrade_is_done_again() {
    for (hard_links, checkpoint) in [
        (true, "FixesApplied"),
        (false, "FixesApplied"),
        (true, "NewWorldAtTopLevel"),
    ] {
        let setup = setup();
        let before = snapshot(&setup.world, &[]);
        kill_at(&setup.saves, hard_links, checkpoint);
        assert_eq!(
            snapshot(&setup.world, &["filefix", "filefix/new_world"])
                .into_iter()
                .filter(|(k, _)| !k.starts_with("filefix/"))
                .collect::<BTreeMap<_, _>>(),
            before,
            "{checkpoint}"
        );
        let again = fixer(Some(TestHooks {
            capabilities: Some(if hard_links {
                HARD_LINKS
            } else {
                NO_HARD_LINKS
            }),
            ..TestHooks::default()
        }));
        let (result, logs) = capture_logs(|| run(&setup, &again, &UpgradeProgress::default()));
        result.unwrap();
        assert!(!logs.iter().any(|l| l.contains("interrupted")), "{logs:?}");
        assert_fixed_world(&setup.world);
    }
}

#[test]
fn an_upgrade_killed_during_the_moves_is_continued_at_the_next_start() {
    for checkpoint in [
        "JournalWritten",
        "MoveFile:1",
        "MoveFile:2",
        "NewWorldAtTopLevel",
    ] {
        let setup = setup();
        let inode_before = inode(&setup.world.join("stats/s.json"));
        kill_at(&setup.saves, false, checkpoint);
        assert!(
            setup
                .world
                .join("filefix/upgrade_in_progress.json")
                .is_file(),
            "{checkpoint}: the moves were journaled before the first one"
        );
        // The world is half moved (the files moved so far are gone from it); its level.dat still says it is old.
        let moved = ["playerdata/p.dat", "resources.zip", "stats/s.json"]
            .iter()
            .filter(|f| !setup.world.join(f).exists())
            .count();
        let expected_moved = match checkpoint {
            "JournalWritten" => 0,
            "MoveFile:1" => 1,
            _ => 2 + usize::from(checkpoint == "NewWorldAtTopLevel"),
        };
        assert_eq!(moved, expected_moved, "{checkpoint}");
        let again = fixer(Some(TestHooks {
            capabilities: Some(NO_HARD_LINKS),
            ..TestHooks::default()
        }));
        let (progress, recorder) = progress();
        let (result, logs) = capture_logs(|| run(&setup, &again, &progress));
        result.unwrap_or_else(|e| panic!("{checkpoint}: {e}"));
        assert!(
            logs.contains(
                &"WARN: Found previously interrupted world upgrade, attempting to continue it"
                    .to_owned()
            ),
            "{checkpoint}: {logs:?}"
        );
        assert_fixed_world(&setup.world);
        assert_eq!(saves_entries(&setup.saves), ["world"]);
        assert_eq!(inode(&setup.world.join("stats/s.json")), inode_before);
        assert_eq!(*recorder.0.lock().unwrap(), ["started", "finished"]);
    }
}

// ---- the journal --------------------------------------------------------------------------------------------------------

#[test]
fn the_journal_is_pretty_printed_json_and_reads_back() {
    use super::fixer::{read_moves, write_moves};
    let tmp = tempfile::tempdir().unwrap();
    let (from, to) = (
        tmp.path().join("world"),
        tmp.path().join("world/filefix/new_world"),
    );
    let moves = vec![
        FileMove {
            from: from.join("stats/s.json"),
            to: to.join("players/stats/s.json"),
        },
        FileMove {
            from: from.join("a b.dat"),
            to: to.join("c&d.dat"),
        },
    ];
    let file = tmp.path().join("upgrade_in_progress.json");
    write_moves(&moves, &from, &to, "world upgraded", &file).unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "{\n  \"moves\": [\n    {\n      \"from\": \"stats/s.json\",\n      \"to\": \"players/stats/s.json\"\n    },\n    {\n      \"from\": \"a b.dat\",\n      \"to\": \"c\\u0026d.dat\"\n    }\n  ],\n  \"upgraded_folder\": \"world upgraded\"\n}"
    );
    let journal = read_moves(&from, &to, &file).unwrap();
    assert_eq!(journal.moves, moves);
    assert_eq!(journal.upgraded_folder.as_deref(), Some("world upgraded"));
    assert!(!tmp.path().join("upgrade_in_progress.json.tmp").exists());

    for (json, message) in [
        (
            r#"{"moves":[{"from":"../x","to":"y"}]}"#,
            "Illegal path traversal: ../x",
        ),
        (
            r#"{"moves":[{"from":"x","to":"/y"}]}"#,
            "Illegal absolute path: /y",
        ),
        (r#"{"moves":[{"from":"","to":"y"}]}"#, "Path is empty: "),
        (r#"{"moves":[{"from":"x"}]}"#, "No key to in MapLike"),
        (r#"{}"#, "No key moves in MapLike"),
    ] {
        fs::write(&file, json).unwrap();
        let e = read_moves(&from, &to, &file).unwrap_err();
        assert_eq!(e.to_string(), message, "{json}");
    }
    fs::write(&file, r#"{"moves":[],"upgraded_folder":"../x"}"#).unwrap();
    assert_eq!(
        read_moves(&from, &to, &file).unwrap_err().to_string(),
        "Illegal folder name: ../x"
    );
    fs::write(&file, r#"{"moves":[]}"#).unwrap();
    assert!(
        read_moves(&from, &to, &file)
            .unwrap()
            .upgraded_folder
            .is_none(),
        "a journal of vanilla"
    );
    fs::write(&file, r#"{"moves": [{"fro"#).unwrap();
    assert!(matches!(
        read_moves(&from, &to, &file).unwrap_err(),
        FileFixError::Unchecked(_)
    ));
}

/// An `IOException` of a read is wrapped in the `AbortedFileFixException`, the unchecked `ReportedNbtException` of a
/// stream that is cut inside the root compound is not.
#[test]
fn a_damaged_file_ends_the_fix_with_the_exception_the_jdk_throws() {
    let damage = |bytes: fn(Vec<u8>) -> Vec<u8>| {
        let setup = setup();
        let file = setup.world.join("data/counter.dat");
        fs::write(&file, bytes(fs::read(&file).unwrap())).unwrap();
        run(&setup, &fixer(None), &UpgradeProgress::default())
            .unwrap_err()
            .to_string()
    };
    let aborted = "net.minecraft.util.filefix.AbortedFileFixException: ";
    assert_eq!(
        damage(|_| vec![1, 2, 3]),
        format!("{aborted}java.util.zip.ZipException: Not in GZIP format")
    );
    assert_eq!(
        damage(|b| b[..12].to_vec()),
        format!("{aborted}java.io.EOFException: Unexpected end of ZLIB input stream")
    );
    assert_eq!(
        damage(|b| b[..b.len() / 2].to_vec()),
        "net.minecraft.nbt.ReportedNbtException: Loading NBT data"
    );
}

#[test]
fn the_latest_file_fixer_version_is_derived_from_the_registered_file_fixes() {
    assert_eq!(mc_datafix::latest_file_fixer_version(), 4899);
    let registered: Vec<_> = mc_datafix::get_file_fixer()
        .fixes()
        .iter()
        .map(|f| f.version)
        .collect();
    assert_eq!(registered, [1493, 4772, 4772, 4772, 4772, 4772, 4773, 4899]);
}

#[test]
fn the_registered_file_fixes_build_the_fixer_and_a_fix_of_another_version_is_refused() {
    let registry = mc_datafix::get_file_fixer();
    let fixer = FileFixerUpper::from_registry(test_data_fixer(), registry, |registration| {
        FileFix::new(registration.version)
    });
    assert!(fixer.requires_file_fixing(4898) && !fixer.requires_file_fixing(4899));
    let refused = std::panic::catch_unwind(|| {
        FileFixerUpper::from_registry(test_data_fixer(), mc_datafix::get_file_fixer(), |_| {
            FileFix::new(1)
        })
    });
    assert!(refused.is_err());
}

#[test]
fn the_capabilities_of_a_file_system_are_tried_in_the_folder_and_leave_nothing_in_it() {
    let tmp = tempfile::tempdir().unwrap();
    let capabilities = detect_file_system_capabilities(tmp.path()).unwrap();
    assert!(capabilities.atomic_move);
    assert!(fs::read_dir(tmp.path()).unwrap().next().is_none());
    assert!(detect_file_system_capabilities(&tmp.path().join("missing")).is_err());
}

#[test]
fn a_journal_that_cannot_be_read_stops_the_start_without_touching_anything() {
    let setup = setup();
    // Leave an interrupted upgrade behind that cannot be reverted, then damage its journal.
    let new_world = setup.world.join("filefix/new_world");
    let sabotage = Arc::new(move |at: Checkpoint| {
        if at == Checkpoint::MoveFile(1) {
            // the one file moved so far (the moves go in `HashMap` order of the names)
            for file in [
                "players/data/p.dat",
                "stats/s.json",
                "resourcepacks/resources.zip",
            ] {
                let _ = fs::remove_file(new_world.join(file));
            }
        }
    });
    let hooks = TestHooks {
        capabilities: Some(NO_HARD_LINKS),
        fail_at: Some(Checkpoint::MoveFile(1)),
        action: Some(sabotage),
        ..TestHooks::default()
    };
    run(&setup, &fixer(Some(hooks)), &UpgradeProgress::default()).unwrap_err();
    let journal = setup.world.join("filefix/upgrade_in_progress.json");
    fs::write(&journal, r#"{"moves": [{"from": "../x""#).unwrap();
    let before = snapshot(&setup.world, &[]);

    let (progress, recorder) = progress();
    let error = run(&setup, &fixer(None), &progress).unwrap_err();
    assert!(matches!(error, FileFixError::Unchecked(_)), "{error:?}");
    assert_eq!(snapshot(&setup.world, &[]), before, "nothing is deleted");
    assert_eq!(*recorder.0.lock().unwrap(), ["started"]);
}

#[test]
fn a_failure_before_the_moves_of_a_continued_upgrade_keeps_what_was_moved() {
    let setup = setup();
    kill_at(&setup.saves, false, "MoveFile:2");
    let moved = setup.world.join("filefix/new_world/players/data/p.dat");
    assert!(
        moved.is_file(),
        "a file moved before the failure is in the new world"
    );

    struct NoCapabilities;
    impl FixHooks for NoCapabilities {
        fn file_system_capabilities(&self, _dir: &Path) -> io::Result<FileSystemCapabilities> {
            Err(io::Error::other("the probe failed"))
        }
    }
    let failing = FileFixerUpper::new(test_data_fixer(), vec![fix_of_version()], LATEST)
        .with_hooks(Box::new(NoCapabilities));
    let (progress, recorder) = progress();
    let error = run(&setup, &failing, &progress).unwrap_err();
    assert!(matches!(error, FileFixError::Aborted { .. }), "{error:?}");
    assert!(
        moved.is_file(),
        "the only copy of a moved file is not deleted"
    );
    assert!(
        setup
            .world
            .join("filefix/upgrade_in_progress.json")
            .is_file()
    );
    assert_eq!(
        *recorder.0.lock().unwrap(),
        [
            "started",
            "failed: The world upgrade failed while swapping in the file-fixed world"
        ]
    );

    // The next start continues and finishes.
    let again = fixer(Some(TestHooks {
        capabilities: Some(NO_HARD_LINKS),
        ..TestHooks::default()
    }));
    run(&setup, &again, &UpgradeProgress::default()).unwrap();
    assert_fixed_world(&setup.world);
}

#[test]
fn a_journal_cannot_name_a_folder_that_is_not_an_upgraded_world() {
    let setup = setup();
    let other = setup.saves.join("OtherWorld");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("level.dat"), b"somebody else's world").unwrap();
    fs::create_dir_all(setup.world.join("filefix")).unwrap();
    fs::write(
        setup.world.join("filefix/upgrade_in_progress.json"),
        r#"{"moves": [], "upgraded_folder": "OtherWorld"}"#,
    )
    .unwrap();
    let before = snapshot(&setup.saves, &[]);
    let error = run(&setup, &fixer(None), &UpgradeProgress::default()).unwrap_err();
    assert!(matches!(error, FileFixError::Aborted { .. }), "{error:?}");
    assert_eq!(
        snapshot(&setup.saves, &[]),
        before,
        "no folder is replaced or deleted"
    );
}

#[test]
fn a_content_fix_can_cancel_and_the_only_file_is_checked() {
    let setup = setup();
    let before = snapshot(&setup.world, &[]);
    let mut canceling = FileFix::new(FIX_VERSION);
    canceling.add_file_content_fix(|_| Err(FileFixError::Canceled));
    let fixer = FileFixerUpper::new(test_data_fixer(), vec![canceling], LATEST);
    let (progress, recorder) = progress();
    let error = run(&setup, &fixer, &progress).unwrap_err();
    assert!(matches!(error, FileFixError::Canceled), "{error:?}");
    assert_eq!(snapshot(&setup.world, &[]), before);
    assert_eq!(*recorder.0.lock().unwrap(), ["started"]);

    assert!(matches!(
        only_file(vec![1, 2]),
        Err(FileFixError::Unchecked(_))
    ));
    assert!(matches!(
        only_file::<i32>(vec![]),
        Err(FileFixError::Unchecked(_))
    ));
    assert_eq!(only_file(vec![7]).unwrap(), 7);
}

#[test]
fn level_dat_player_data_and_saved_data_are_read_and_written_in_the_view() {
    let tmp = tempfile::tempdir().unwrap();
    let world = tmp.path().join("world");
    fs::create_dir_all(world.join("players/data")).unwrap();
    write_compressed(&level_dat(FIX_VERSION), &world.join("level.dat")).unwrap();
    let mut player = CompoundTag::new();
    player.put_int("DataVersion", FIX_VERSION);
    player.put_int("Health", 20);
    write_compressed(&player, &world.join("players/data/p.dat")).unwrap();
    let before = snapshot(&world, &[]);
    let mut tree = CowTree::create(&world, &tmp.path().join("cow"), |_| false).unwrap();

    let level = LevelDat::new(PathBuf::from("level.dat"), FIX_VERSION);
    let data = level.read(&tree).unwrap().expect("level.dat exists");
    assert_eq!(data.get("LevelName").as_string_or(""), "world");
    level
        .write(
            &mut tree,
            &data.set("LevelName", data.create_string("renamed")),
        )
        .unwrap();
    let data = level.read(&tree).unwrap().unwrap();
    assert_eq!(data.get("LevelName").as_string_or(""), "renamed");
    assert_eq!(data.get("DataVersion").as_int_or(0), FIX_VERSION);

    let player_file = PlayerData::new(PathBuf::from("players/data/p.dat"), FIX_VERSION);
    let player = player_file.read(&tree).unwrap().unwrap();
    assert_eq!(player.get("Health").as_int_or(0), 20);
    player_file
        .write(&mut tree, &player.set("Health", player.create_int(10)))
        .unwrap();
    assert_eq!(
        player_file
            .read(&tree)
            .unwrap()
            .unwrap()
            .get("Health")
            .as_int_or(0),
        10
    );

    // A missing file reads as `None` (and logs), a new one is created together with its folders.
    let missing = PlayerData::new(PathBuf::from("players/data/other.dat"), FIX_VERSION);
    let (read, logs) = capture_logs(|| missing.read(&tree).unwrap());
    assert!(read.is_none());
    assert_eq!(logs, ["INFO: Missing file: /players/data/other.dat"]);
    let saved = SavedDataNbt::new(
        References::SAVED_DATA_GAME_RULES,
        PathBuf::from("dimensions/minecraft/the_end/data/minecraft/rules.dat"),
        FIX_VERSION,
        MissingSeverity::Minor,
    );
    let rules = Dynamic::new(NBT, Dyn::from(&Tag::Compound(CompoundTag::new())));
    saved
        .write(&mut tree, &rules.set("n", rules.create_int(3)))
        .unwrap();
    assert_eq!(saved.read(&tree).unwrap().unwrap().get("n").as_int_or(0), 3);

    assert_eq!(snapshot(&world, &[]), before, "the world is not changed");
}

#[test]
fn an_oversized_chunk_of_a_region_file_is_updated_through_its_external_file() {
    let tmp = tempfile::tempdir().unwrap();
    let world = tmp.path().join("world");
    fs::create_dir_all(world.join("region")).unwrap();
    // More than the 255 sectors a region file holds itself: the chunk goes to `c.1.2.mcc`.
    let mut noise = Vec::with_capacity(1_300_000);
    let mut state = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..1_300_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        noise.push((state >> 24) as i8);
    }
    let mut chunk = CompoundTag::new();
    chunk.put_int("DataVersion", mc_datafix::CURRENT_VERSION);
    chunk.put_byte_array("noise", noise.clone());
    let mut storage = RegionFileStorage::new(region_info(), world.join("region"), false);
    storage.write(&chunk_pos(), Some(&chunk)).unwrap();
    storage.close().unwrap();
    assert!(world.join("region/c.1.2.mcc").is_file());
    let original = fs::read(world.join("region/c.1.2.mcc")).unwrap();

    let mut tree = CowTree::create(&world, &tmp.path().join("cow"), |_| false).unwrap();
    let mut chunks = ChunkNbt::new(
        &mut tree,
        region_info(),
        PathBuf::from("region"),
        DataFixTypes::CHUNK,
        FIX_VERSION,
    )
    .unwrap();
    chunks
        .update_chunk(&tree, &chunk_pos(), None, |mut tag| {
            tag.put_int("touched", 1);
            tag
        })
        .unwrap();
    chunks.close(&mut tree).unwrap();
    assert_eq!(
        fs::read(world.join("region/c.1.2.mcc")).unwrap(),
        original,
        "the world is not changed"
    );

    let moves = tree.collect_move_operations(&tmp.path().join("out"));
    assert_eq!(
        moves.copied_files.len(),
        2,
        "the region file and its external file"
    );
    fs::create_dir_all(tmp.path().join("out/region")).unwrap();
    move_files_for_test(&moves);
    let mut storage = RegionFileStorage::new(region_info(), tmp.path().join("out/region"), false);
    let updated = storage.read(&chunk_pos()).unwrap().unwrap();
    assert_eq!(updated.get_int_or("touched", 0), 1);
    assert_eq!(
        updated.get_byte_array("noise").map(<[i8]>::len),
        Some(noise.len())
    );
}

fn move_files_for_test(moves: &Moves) {
    for m in &moves.copied_files {
        fs::rename(&m.from, &m.to).unwrap();
    }
}

#[test]
fn a_kill_after_the_old_world_moved_out_leaves_only_the_upgraded_world_listed() {
    for hard_links in [true, false] {
        let setup = setup();
        kill_at(&setup.saves, hard_links, "OldWorldMovedOut");
        assert_eq!(
            saves_entries(&setup.saves),
            ["world OUTDATED", "world upgraded"],
            "the old world is out of the way, the new one not yet in"
        );
        for file in ["level.dat", "level.dat_old"] {
            assert!(
                !setup.saves.join("world OUTDATED").join(file).exists(),
                "{file}: a folder that is no longer the world is not one that a world list shows"
            );
        }
        assert!(setup.saves.join("world upgraded/level.dat").is_file());
    }
}

#[test]
fn only_the_names_the_swap_makes_of_a_world_name_are_trusted() {
    use super::names::is_available_name_of;
    let base = "world upgraded";
    for (candidate, expected) in [
        ("world upgraded", true),
        ("world upgraded (3)", true),
        ("world upgraded (12)", true),
        ("world upgraded ()", false),
        ("world upgraded (x)", false),
        ("world upgraded (1", false),
        ("world upgraded 2", false),
        ("OtherWorld", false),
        ("../world upgraded", false),
    ] {
        assert_eq!(
            is_available_name_of(base, candidate),
            expected,
            "{candidate}"
        );
    }
    // A name of 255 units or more is cut, with room left for the counter.
    let long = format!("{} upgraded", "x".repeat(250));
    assert!(is_available_name_of(
        &long,
        &format!("{} upgr", "x".repeat(250))
    ));
    assert!(!is_available_name_of(&long, &"x".repeat(255)));
    assert!(is_available_name_of(
        &long,
        &format!("{} (12)", "x".repeat(250))
    ));
    assert!(!is_available_name_of(
        &long,
        &format!("{} (12)", "x".repeat(255))
    ));
    assert!(!is_available_name_of(&long, &long));
}

#[test]
fn an_old_world_that_cannot_be_moved_out_is_deleted_instead() {
    let setup = setup();
    let blocked = setup.saves.join("world OUTDATED");
    let block = Arc::new(move |at: Checkpoint| {
        // The name was free when it was picked; now a folder with content is in the way of the rename.
        if at == Checkpoint::NewWorldAtTopLevel {
            fs::create_dir_all(blocked.join("in the way")).unwrap();
        }
    });
    let hooks = TestHooks {
        capabilities: Some(HARD_LINKS),
        action: Some(block),
        ..TestHooks::default()
    };
    let (result, logs) =
        capture_logs(|| run(&setup, &fixer(Some(hooks)), &UpgradeProgress::default()));
    result.unwrap();
    assert_fixed_world(&setup.world);
    // The folder of the old name is deleted at the end, whatever is in it.
    assert_eq!(saves_entries(&setup.saves), ["world"]);
    assert!(
        logs.iter().any(|l| l.starts_with(
            "WARN: Failed to move outdated world folder out of the way; will try to delete instead: "
        )),
        "{logs:?}"
    );
}
