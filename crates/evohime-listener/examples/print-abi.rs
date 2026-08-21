//! Печатает раскладку ABI, которую зеркалит `engine::whisper_dll`.
//!
//! Нужен генератору поставки (`scripts/build-listener-runtime.ps1`): он
//! сверяет эти числа с `sizeof` из заголовков собранной whisper.dll. Без
//! такой сверки несовпадение раскладки дошло бы до пользователя в виде
//! `abi_unsupported` после загрузки почти гигабайта, а не остановило бы
//! сборку.
//!
//! Пример, а не бинарь крейта: в продукт этот код не входит.

#[cfg(windows)]
fn main() {
    use evohime_listener::engine::whisper_dll::{mirrored_sizes, ABI_TOKEN};

    let (context_params_size, full_params_size) = mirrored_sizes();
    println!(
        "{{\"name\":\"{ABI_TOKEN}\",\"context_params_size\":{context_params_size},\"full_params_size\":{full_params_size}}}"
    );
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Раскладка ABI зеркалится только для Windows.");
    std::process::exit(1);
}
