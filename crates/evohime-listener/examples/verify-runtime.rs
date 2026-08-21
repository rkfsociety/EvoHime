//! Проверяет собранный каталог рантайма тем же кодом, что и листенер.
//!
//! Генератор поставки (`scripts/build-listener-runtime.ps1`) вызывает этот
//! пример последним шагом. Смысл в том, чтобы расхождение между манифестом и
//! каталогом обнаруживалось на сборке, а не у пользователя: там оно выглядит
//! как отказ движка с кодом, и разбирать его уже негде.
//!
//! Пример, а не бинарь крейта: в продукт этот код не входит.

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;

    use evohime_listener::engine::whisper_dll::verify_abi;
    use evohime_listener::tools_dir::load;

    let Some(root) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Использование: verify-runtime <каталог рантайма>");
        std::process::exit(2);
    };

    let runtime = match load(&root) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Каталог рантайма отвергнут: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = verify_abi(&runtime.manifest) {
        eprintln!("Раскладка ABI отвергнута: {error}");
        std::process::exit(1);
    }

    let rungs: Vec<&str> = runtime.models.keys().map(|rung| rung.as_str()).collect();
    let missing: Vec<&str> = runtime
        .missing_optional
        .iter()
        .map(|role| role.as_str())
        .collect();
    println!(
        "версия {}, ступени: {}, отсутствуют опциональные: {}",
        runtime.manifest.version,
        rungs.join(", "),
        if missing.is_empty() {
            "нет".to_string()
        } else {
            missing.join(", ")
        }
    );
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Рантайм листенера проверяется только на Windows.");
    std::process::exit(1);
}
