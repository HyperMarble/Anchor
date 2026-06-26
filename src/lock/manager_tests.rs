use super::*;
use std::path::Path;
use std::sync::Arc;
use std::thread;

#[test]
fn test_basic_lock_unlock() {
    let manager = LockManager::new();
    let result = manager.try_acquire(Path::new("test.rs"));
    assert!(matches!(result, LockResult::Acquired { .. }));
    assert!(manager.is_locked(Path::new("test.rs")));
    manager.release(Path::new("test.rs"));
    assert!(!manager.is_locked(Path::new("test.rs")));
}

#[test]
fn test_double_lock_blocked() {
    let manager = LockManager::new();
    let _r1 = manager.try_acquire(Path::new("test.rs"));
    let r2 = manager.try_acquire(Path::new("test.rs"));
    assert!(matches!(r2, LockResult::Blocked { .. }));
}

#[test]
fn test_different_files_ok() {
    let manager = LockManager::new();
    let r1 = manager.try_acquire(Path::new("a.rs"));
    let r2 = manager.try_acquire(Path::new("b.rs"));
    assert!(matches!(r1, LockResult::Acquired { .. }));
    assert!(matches!(r2, LockResult::Acquired { .. }));
}

#[test]
fn test_symbol_lock_independent() {
    let manager = LockManager::new();
    let foo = SymbolKey::new("test.rs", "foo");
    let bar = SymbolKey::new("test.rs", "bar");
    let r1 = manager.try_acquire_symbol_simple(&foo);
    let r2 = manager.try_acquire_symbol_simple(&bar);
    assert!(matches!(r1, LockResult::Acquired { .. }));
    assert!(matches!(r2, LockResult::Acquired { .. }));
}

#[test]
fn test_symbol_release() {
    let manager = LockManager::new();
    let foo = SymbolKey::new("test.rs", "foo");
    let _r1 = manager.try_acquire_symbol_simple(&foo);
    manager.release_symbol(&foo);
    let r2 = manager.try_acquire_symbol_simple(&foo);
    assert!(matches!(r2, LockResult::Acquired { .. }));
}

#[test]
fn test_wait_for_lock() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let manager = Arc::new(LockManager::new());
    let lock_acquired = Arc::new(AtomicBool::new(false));

    let m1 = manager.clone();
    let acquired1 = lock_acquired.clone();
    let t1 = thread::spawn(move || {
        let _result = m1.try_acquire(Path::new("/tmp/test_lock_wait.rs"));
        acquired1.store(true, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(100));
        m1.release(Path::new("/tmp/test_lock_wait.rs"));
    });

    while !lock_acquired.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(5));
    }

    let result = manager.acquire_with_wait(
        Path::new("/tmp/test_lock_wait.rs"),
        Duration::from_millis(500),
    );
    t1.join().unwrap();

    assert!(
        matches!(
            result,
            LockResult::Acquired { .. } | LockResult::AcquiredAfterWait { .. }
        ),
        "Should have acquired lock after waiting"
    );
}
