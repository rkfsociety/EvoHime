//! Дополнительная проверка подписи файлов рантайма.
//!
//! Основной корень доверия — SHA-256 из манифеста релизного канала
//! (см. [`crate::tools_dir`]). Подпись проверяется ровно у тех файлов, у
//! которых она бывает: `onnxruntime.dll` подписан Microsoft, и для него
//! отсутствие или недоверенная подпись — отказ.
//!
//! Собственный `whisper.dll` подписан **не будет**, пока в проекте не появится
//! настоящий signing pipeline: в `.github/workflows/windows.yml` нет ни
//! `signtool`, ни сертификата, а в `electron-builder.yml` нет `certificateFile`.
//! Требовать подпись у своих артефактов сейчас означало бы предъявить
//! требование, которого не выполняет и сам продукт, поэтому неподписанный
//! `whisper.dll` — штатное состояние, зафиксированное тестом, а не недосмотр.

use std::path::Path;

use crate::engine::EngineUnavailable;
use crate::tools_dir::FileRole;

/// Что удалось узнать о подписи файла.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignatureStatus {
    /// Подпись есть и цепочка доверена системой.
    Trusted { microsoft: bool },
    /// Подписи нет вовсе.
    Unsigned,
    /// Подпись есть, но цепочка не проходит проверку.
    Untrusted,
    /// Проверить не удалось (нет платформенного API). Отдельное значение, а не
    /// «доверяем»: неизвестность не приравнивается к успеху.
    Unknown,
}

/// Файлы, для которых подпись обязательна.
pub const fn signature_required(role: FileRole) -> bool {
    matches!(role, FileRole::OnnxRuntimeDll)
}

/// Решение по одной паре «роль + состояние подписи».
///
/// Вынесено в чистую функцию, чтобы политика проверялась тестами на любой
/// платформе, а не только там, где есть WinVerifyTrust.
pub fn decide(role: FileRole, status: &SignatureStatus) -> Result<(), EngineUnavailable> {
    if !signature_required(role) {
        return Ok(());
    }
    match status {
        SignatureStatus::Trusted { microsoft: true } => Ok(()),
        SignatureStatus::Trusted { microsoft: false } => Err(EngineUnavailable::SignatureUntrusted),
        SignatureStatus::Untrusted => Err(EngineUnavailable::SignatureUntrusted),
        SignatureStatus::Unsigned => Err(EngineUnavailable::SignatureMissing),
        SignatureStatus::Unknown => Err(EngineUnavailable::SignatureMissing),
    }
}

/// Проверяет файл, если его роль этого требует.
pub fn require(role: FileRole, path: &Path) -> Result<(), EngineUnavailable> {
    if !signature_required(role) {
        return Ok(());
    }
    decide(role, &inspect(path))
}

/// Подстрока, по которой распознаётся издатель. Точное имя субъекта меняется
/// от сертификата к сертификату (`Microsoft Corporation`, `Microsoft
/// Windows`), поэтому сравнивается общая часть.
const MICROSOFT_SUBJECT: &str = "microsoft";

/// Читает состояние подписи файла.
#[cfg(windows)]
pub fn inspect(path: &Path) -> SignatureStatus {
    match verify_chain(path) {
        ChainResult::Trusted => SignatureStatus::Trusted {
            microsoft: signer_subject(path)
                .map(|subject| subject.to_lowercase().contains(MICROSOFT_SUBJECT))
                .unwrap_or(false),
        },
        ChainResult::Unsigned => SignatureStatus::Unsigned,
        ChainResult::Untrusted => SignatureStatus::Untrusted,
    }
}

#[cfg(not(windows))]
pub fn inspect(_path: &Path) -> SignatureStatus {
    SignatureStatus::Unknown
}

#[cfg(windows)]
enum ChainResult {
    Trusted,
    Unsigned,
    Untrusted,
}

#[cfg(windows)]
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// `WinVerifyTrust` с политикой Authenticode и без всякого UI.
#[cfg(windows)]
fn verify_chain(path: &Path) -> ChainResult {
    use windows_sys::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
        WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
        WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };

    // Значение из winerror.h: подписи в файле нет вообще. Отличать его от
    // «подпись есть, но плохая» обязательно — это разные сообщения и разные
    // причины отказа.
    const TRUST_E_NOSIGNATURE: i32 = -2146762496; // 0x800B0100
    const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = -2146762477; // 0x800B0003
    const TRUST_E_PROVIDER_UNKNOWN: i32 = -2146762478; // 0x800B0001

    let wide_path = wide(path);
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: wide_path.as_ptr(),
        hFile: std::ptr::null_mut(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: std::ptr::null_mut(),
        pSIPClientData: std::ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: std::ptr::null_mut(),
        pwszURLReference: std::ptr::null_mut(),
        dwProvFlags: 0,
        dwUIContext: 0,
        pSignatureSettings: std::ptr::null_mut(),
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }
    match status {
        0 => ChainResult::Trusted,
        TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => {
            ChainResult::Unsigned
        }
        _ => ChainResult::Untrusted,
    }
}

/// Простое отображаемое имя субъекта первого подписанта.
#[cfg(windows)]
fn signer_subject(path: &Path) -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{
        CertCloseStore, CertFindCertificateInStore, CertFreeCertificateContext, CertGetNameStringW,
        CryptMsgClose, CryptMsgGetParam, CryptQueryObject, CERT_FIND_SUBJECT_CERT,
        CERT_NAME_SIMPLE_DISPLAY_TYPE, CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
        CERT_QUERY_FORMAT_FLAG_BINARY, CERT_QUERY_OBJECT_FILE, CMSG_SIGNER_CERT_INFO_PARAM,
        PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
    };

    let wide_path = wide(path);
    let mut encoding = 0;
    let mut content_type = 0;
    let mut format_type = 0;
    let mut store = std::ptr::null_mut();
    let mut message = std::ptr::null_mut();
    let queried = unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide_path.as_ptr().cast(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            &mut encoding,
            &mut content_type,
            &mut format_type,
            &mut store,
            &mut message,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return None;
    }
    let subject = unsafe { read_subject(store, message, encoding) };
    unsafe {
        if !message.is_null() {
            CryptMsgClose(message);
        }
        if !store.is_null() {
            CertCloseStore(store, 0);
        }
    }
    return subject;

    /// # Safety
    /// `store` и `message` — валидные дескрипторы, полученные из
    /// `CryptQueryObject` и живущие до возврата.
    unsafe fn read_subject(
        store: windows_sys::Win32::Security::Cryptography::HCERTSTORE,
        message: *mut core::ffi::c_void,
        encoding: u32,
    ) -> Option<String> {
        let mut size = 0u32;
        if CryptMsgGetParam(
            message,
            CMSG_SIGNER_CERT_INFO_PARAM,
            0,
            std::ptr::null_mut(),
            &mut size,
        ) == 0
            || size == 0
        {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        if CryptMsgGetParam(
            message,
            CMSG_SIGNER_CERT_INFO_PARAM,
            0,
            buffer.as_mut_ptr().cast(),
            &mut size,
        ) == 0
        {
            return None;
        }
        let encoding = if encoding == 0 {
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING
        } else {
            encoding
        };
        let context = CertFindCertificateInStore(
            store,
            encoding,
            0,
            CERT_FIND_SUBJECT_CERT,
            buffer.as_ptr().cast(),
            std::ptr::null(),
        );
        if context.is_null() {
            return None;
        }
        let needed = CertGetNameStringW(
            context,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
            0,
        );
        let subject = if needed <= 1 {
            None
        } else {
            let mut name = vec![0u16; needed as usize];
            let written = CertGetNameStringW(
                context,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null(),
                name.as_mut_ptr(),
                needed,
            );
            if written <= 1 {
                None
            } else {
                Some(String::from_utf16_lossy(&name[..(written - 1) as usize]))
            }
        };
        CertFreeCertificateContext(context);
        subject
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_microsoft_runtime_needs_a_signature() {
        assert!(signature_required(FileRole::OnnxRuntimeDll));
        assert!(!signature_required(FileRole::WhisperDll));
        assert!(!signature_required(FileRole::SileroVad));
    }

    /// Осознанное поведение, а не пропуск: у собственных артефактов проекта
    /// подписи нет, потому что подписывать их пока нечем.
    #[test]
    fn unsigned_own_dll_is_accepted() {
        assert_eq!(
            decide(FileRole::WhisperDll, &SignatureStatus::Unsigned),
            Ok(())
        );
        assert_eq!(
            decide(FileRole::WhisperDll, &SignatureStatus::Untrusted),
            Ok(())
        );
        assert_eq!(
            decide(FileRole::WhisperDll, &SignatureStatus::Unknown),
            Ok(())
        );
    }

    #[test]
    fn onnxruntime_without_a_valid_microsoft_signature_is_rejected() {
        assert_eq!(
            decide(FileRole::OnnxRuntimeDll, &SignatureStatus::Unsigned),
            Err(EngineUnavailable::SignatureMissing)
        );
        assert_eq!(
            decide(FileRole::OnnxRuntimeDll, &SignatureStatus::Untrusted),
            Err(EngineUnavailable::SignatureUntrusted)
        );
        assert_eq!(
            decide(
                FileRole::OnnxRuntimeDll,
                &SignatureStatus::Trusted { microsoft: false }
            ),
            Err(EngineUnavailable::SignatureUntrusted)
        );
        assert_eq!(
            decide(
                FileRole::OnnxRuntimeDll,
                &SignatureStatus::Trusted { microsoft: true }
            ),
            Ok(())
        );
    }

    /// Невозможность проверить не превращается в «проверено».
    #[test]
    fn unknown_signature_state_is_not_success() {
        assert_eq!(
            decide(FileRole::OnnxRuntimeDll, &SignatureStatus::Unknown),
            Err(EngineUnavailable::SignatureMissing)
        );
    }

    /// Ручная проверка на реальном файле: `EVOHIME_LISTENER_SIGNATURE_FILE`
    /// указывает на подписанный файл, и тест печатает распознанное состояние.
    #[cfg(windows)]
    #[test]
    fn inspects_a_real_file_when_asked() {
        let Some(path) = std::env::var_os("EVOHIME_LISTENER_SIGNATURE_FILE") else {
            return;
        };
        let status = inspect(std::path::Path::new(&path));
        assert_ne!(status, SignatureStatus::Unknown);
    }
}
