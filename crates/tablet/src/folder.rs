//! The music folder the user picked, and its iCloud Drive tracks.
//!
//! An iPadOS app sees only its own Documents folder, unless the user picks
//! another with the system's folder picker. The pick grants access to the
//! folder and everything in it, and a bookmark of it keeps that access
//! across launches. A folder in iCloud Drive holds tracks that may not be
//! on the iPad yet: they are listed by name and downloaded in the
//! background.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A folder the picker returned, waiting for the frame timer.
static PICKED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Takes the folder the user just picked, if any.
pub fn take_picked() -> Option<PathBuf> {
    PICKED.lock().ok()?.take()
}

/// Whether this platform has a folder picker (the desktop uses
/// `OPENDJ_MUSIC`).
pub const CAN_PICK: bool = cfg!(target_os = "ios");

/// Where the bookmark of the picked folder is kept.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn bookmark_file() -> Option<PathBuf> {
    Some(
        opendj_engine::paths::config_base()?
            .join(opendj_engine::paths::app_folder_name())
            .join("tablet-folder.bookmark"),
    )
}

/// An iCloud Drive placeholder's track: `.Song.mp3.icloud` stands for
/// `Song.mp3`, which isn't on the device yet.
pub fn placeholder_for(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let real = name.strip_prefix('.')?.strip_suffix(".icloud")?;
    let real = path.with_file_name(real);
    opendj_engine::scan::is_audio(&real).then_some(real)
}

/// The audio files under `dir`, and the ones among them that are still in
/// iCloud (as placeholders, or dataless files).
pub fn audio_files(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut cloud = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if let Some(real) = placeholder_for(&path) {
                cloud.push(real.clone());
                files.push(real);
            } else if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            } else if kind.is_dir() && depth < 12 {
                stack.push((path, depth + 1));
            } else if kind.is_file() && opendj_engine::scan::is_audio(&path) {
                if opendj_engine::scan::is_cloud_only(&path) {
                    cloud.push(path.clone());
                }
                files.push(path);
            }
        }
        if files.len() >= opendj_engine::scan::MAX_FILES {
            break;
        }
    }
    files.sort();
    files.dedup();
    (files, cloud)
}

/// Whether `path` is on the device and can be played.
pub fn is_here(path: &Path) -> bool {
    path.is_file() && !opendj_engine::scan::is_cloud_only(path)
}

#[cfg(target_os = "ios")]
pub use ios::{download, pick, restore};

#[cfg(target_os = "ios")]
mod ios {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use objc2::rc::Retained;
    use objc2::runtime::{Bool, ProtocolObject};
    use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
    use objc2_foundation::{
        NSArray, NSData, NSFileManager, NSObject, NSObjectProtocol, NSString,
        NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL,
    };
    use objc2_ui_kit::{
        UIApplication, UIDocumentPickerDelegate, UIDocumentPickerViewController, UIViewController,
        UIWindowScene,
    };
    use objc2_uniform_type_identifiers::UTTypeFolder;

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and the
        // delegate has no ivars.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "OpenDJFolderPickerDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UIDocumentPickerDelegate for Delegate {
            #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
            fn did_pick(&self, _picker: &UIDocumentPickerViewController, urls: &NSArray<NSURL>) {
                if let Some(url) = urls.firstObject() {
                    picked(&url);
                }
            }
        }
    );

    thread_local! {
        /// The picker holds its delegate weakly.
        static DELEGATE: RefCell<Option<Retained<Delegate>>> = const { RefCell::new(None) };
    }

    fn path_of(url: &NSURL) -> Option<PathBuf> {
        url.path().map(|p| PathBuf::from(p.to_string()))
    }

    /// Keeps access to the folder and remembers it for the next launch.
    fn picked(url: &NSURL) {
        // SAFETY: the URL came from the document picker.
        unsafe { url.startAccessingSecurityScopedResource() };
        save(url);
        if let (Some(path), Ok(mut slot)) = (path_of(url), super::PICKED.lock()) {
            *slot = Some(path);
        }
    }

    fn save(url: &NSURL) {
        let Some(file) = super::bookmark_file() else {
            return;
        };
        match url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions(0),
            None,
            None,
        ) {
            Ok(data) => {
                let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
                if let Err(e) = std::fs::write(&file, data.to_vec()) {
                    eprintln!("folder bookmark: {e}");
                }
            }
            Err(e) => eprintln!("folder bookmark: {}", e.localizedDescription()),
        }
    }

    /// The folder picked on an earlier launch, with access to it again.
    pub fn restore() -> Option<PathBuf> {
        let bytes = std::fs::read(super::bookmark_file()?).ok()?;
        let data = NSData::with_bytes(&bytes);
        let mut stale = Bool::NO;
        // SAFETY: `stale` outlives the call.
        let url = match unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                NSURLBookmarkResolutionOptions(0),
                None,
                &mut stale,
            )
        } {
            Ok(url) => url,
            Err(e) => {
                eprintln!("folder bookmark: {}", e.localizedDescription());
                return None;
            }
        };
        // SAFETY: the URL came from a bookmark the picker granted.
        if !unsafe { url.startAccessingSecurityScopedResource() } {
            eprintln!("folder bookmark: no access");
        }
        if stale.as_bool() {
            save(&url);
        }
        path_of(&url)
    }

    /// The view controller the app's window shows, to present from.
    fn root_view_controller(mtm: MainThreadMarker) -> Option<Retained<UIViewController>> {
        let app = UIApplication::sharedApplication(mtm);
        for scene in app.connectedScenes().iter() {
            let Ok(scene) = scene.downcast::<UIWindowScene>() else {
                continue;
            };
            if let Some(vc) = scene.windows().iter().find_map(|w| w.rootViewController()) {
                return Some(vc);
            }
        }
        None
    }

    /// Opens the system's folder picker.
    pub fn pick() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(root) = root_view_controller(mtm) else {
            eprintln!("folder picker: no window");
            return;
        };
        // SAFETY: UTTypeFolder is a constant UIKit defines.
        let folder = unsafe { UTTypeFolder };
        let types = NSArray::from_slice(&[folder]);
        let picker =
            UIDocumentPickerViewController::initForOpeningContentTypes(mtm.alloc(), &types);
        // SAFETY: NSObject's init.
        let delegate: Retained<Delegate> = unsafe { msg_send![mtm.alloc::<Delegate>(), init] };
        picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));
        root.presentViewController_animated_completion(&picker, true, None);
    }

    /// Asks iCloud Drive to bring `path` onto the iPad.
    pub fn download(path: &Path) {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        if let Err(e) =
            NSFileManager::defaultManager().startDownloadingUbiquitousItemAtURL_error(&url)
        {
            eprintln!(
                "iCloud download {}: {}",
                path.display(),
                e.localizedDescription()
            );
        }
    }
}

/// No picker on the desktop.
#[cfg(not(target_os = "ios"))]
pub fn pick() {}

/// No saved folder on the desktop.
#[cfg(not(target_os = "ios"))]
pub fn restore() -> Option<PathBuf> {
    None
}

/// On the desktop reading a file brings it down.
#[cfg(not(target_os = "ios"))]
pub fn download(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_icloud_placeholders() {
        let p = Path::new("/music/House/.Song One.mp3.icloud");
        assert_eq!(
            placeholder_for(p),
            Some(PathBuf::from("/music/House/Song One.mp3"))
        );
        assert_eq!(placeholder_for(Path::new("/music/.notes.txt.icloud")), None);
        assert_eq!(placeholder_for(Path::new("/music/Song.mp3")), None);
    }

    #[test]
    fn lists_placeholders_as_tracks_in_icloud() {
        let dir = std::env::temp_dir().join(format!("opendj-tablet-folder-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("here.mp3"), b"x").unwrap();
        std::fs::write(dir.join("sub/.away.flac.icloud"), b"x").unwrap();
        std::fs::write(dir.join(".DS_Store"), b"x").unwrap();
        let (files, cloud) = audio_files(&dir);
        assert_eq!(files, vec![dir.join("here.mp3"), dir.join("sub/away.flac")]);
        assert_eq!(cloud, vec![dir.join("sub/away.flac")]);
        assert!(is_here(&dir.join("here.mp3")));
        assert!(!is_here(&dir.join("sub/away.flac")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
