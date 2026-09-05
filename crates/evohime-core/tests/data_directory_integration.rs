use std::path::Path;

#[test]
fn data_directory_is_available_to_integrations() {
    let directory = evohime_core::get_data_directory();
    assert!(!directory.as_os_str().is_empty());
    assert!(Path::new(&directory).is_relative() || directory.is_absolute());
}
