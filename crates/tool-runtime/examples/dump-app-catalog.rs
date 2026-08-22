//! Печатает каталог приложений так, каким его видит `app.open`.
//!
//! Диагностика для одного вопроса: почему Ева не нашла приложение, которое
//! точно установлено. Из вывода видно и то, что каталог о нём знает, и под
//! каким названием, и какой путь будет запущен.
//!
//! ```text
//! cargo run -p evohime-tool-runtime --example dump-app-catalog
//! ```

fn main() {
    let catalog = evohime_tool_runtime::app_catalog::default_catalog();
    println!("записей: {}", catalog.entries().len());
    for entry in catalog.entries() {
        println!(
            "{}\t{}\t{}\t[{}]",
            entry.id,
            entry.title,
            entry.exec.display(),
            entry.aliases.join(", ")
        );
    }
}
