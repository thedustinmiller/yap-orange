//! Pure function tests for tree export/import.
//!
//! Integration tests requiring a database have moved to:
//! crates/yap-store-pg/tests/export_tests.rs

use yap_core::export::compute_export_hash;

#[test]
fn test_export_hash_stable() {
    let h1 = compute_export_hash("content", "hello world", &[]);
    let h2 = compute_export_hash("content", "hello world", &[]);
    assert_eq!(h1, h2, "same inputs → same hash");

    let h3 = compute_export_hash("content", "different content", &[]);
    assert_ne!(h1, h3, "different content → different hash");

    let h4 = compute_export_hash("todo", "hello world", &[]);
    assert_ne!(h1, h4, "different content_type → different hash");

    let h5 = compute_export_hash("content", "See {0} and {1}", &[0, 1]);
    let h6 = compute_export_hash("content", "See {0} and {1}", &[1, 0]);
    assert_eq!(
        h5, h6,
        "sorted internal IDs → same hash regardless of input order"
    );

    let h7 = compute_export_hash("content", "See {0} and {1}", &[0, 2]);
    assert_ne!(h5, h7, "different link targets → different hash");
}
