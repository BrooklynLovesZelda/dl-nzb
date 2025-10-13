use crate::error::{DlNzbError, PostProcessingError};
use std::ffi::CString;
use std::path::Path;
use std::sync::{Arc, Mutex};

type Result<T> = std::result::Result<T, DlNzbError>;

/// PAR2 operation type
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Par2Operation {
    Scanning = 0,
    Loading = 1,
    Verifying = 2,
    Repairing = 3,
}

impl Par2Operation {
    fn from_u8(val: u8) -> Self {
        match val {
            0 => Par2Operation::Scanning,
            1 => Par2Operation::Loading,
            2 => Par2Operation::Verifying,
            3 => Par2Operation::Repairing,
            _ => Par2Operation::Scanning,
        }
    }
}

// Progress callback type: takes (operation, current, total)
pub type ProgressCallback = Arc<dyn Fn(Par2Operation, u64, u64) + Send + Sync>;

// Global callback storage for FFI bridge
static PROGRESS_CALLBACK: Mutex<Option<ProgressCallback>> = Mutex::new(None);

// C-compatible trampoline function that calls the Rust callback
extern "C" fn progress_trampoline(operation: u8, current: u64, total: u64) {
    if let Ok(guard) = PROGRESS_CALLBACK.lock() {
        if let Some(callback) = guard.as_ref() {
            callback(Par2Operation::from_u8(operation), current, total);
        }
    }
}

// Manual FFI declarations following Rust Nomicon approach
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Par2Result {
    Success = 0,
    RepairPossible = 1,
    RepairNotPossible = 2,
    InvalidArguments = 3,
    InsufficientData = 4,
    RepairFailed = 5,
    FileIOError = 6,
    LogicError = 7,
    MemoryError = 8,
}

// C function pointer type for progress callback
type CProgressCallback = extern "C" fn(operation: u8, current: u64, total: u64);

// External C function declarations
unsafe extern "C" {
    fn par2_repair_sync(parfilename: *const std::os::raw::c_char, do_repair: bool) -> Par2Result;

    fn par2_repair_with_progress(
        parfilename: *const std::os::raw::c_char,
        do_repair: bool,
        purge_files: bool,
        progress_callback: Option<CProgressCallback>,
    ) -> Par2Result;
}

/// Rust wrapper for PAR2 repair functionality
pub struct Par2Repairer {
    par2_file: String,
}

impl Par2Repairer {
    /// Create a new PAR2 repairer for the given PAR2 file
    pub fn new(par2_file: &Path) -> Result<Self> {
        Ok(Self {
            par2_file: par2_file.to_string_lossy().to_string(),
        })
    }

    /// Perform PAR2 repair or verification (synchronous, single-threaded)
    ///
    /// # Arguments
    /// * `do_repair` - If true, perform repair; if false, only verify
    ///
    /// # Returns
    /// * `Ok(())` - Files were correct or successfully repaired
    /// * `Err(DlNzbError)` - Repair failed or not possible
    pub fn repair(&self, do_repair: bool) -> Result<()> {
        self.repair_with_progress(do_repair, false, None)
    }

    /// Perform PAR2 repair or verification with progress callback
    ///
    /// # Arguments
    /// * `do_repair` - If true, perform repair; if false, only verify
    /// * `purge_files` - If true, delete PAR2 files after successful repair
    /// * `progress_callback` - Optional callback for progress updates (operation, current, total)
    ///
    /// # Returns
    /// * `Ok(())` - Files were correct or successfully repaired
    /// * `Err(DlNzbError)` - Repair failed or not possible
    pub fn repair_with_progress(
        &self,
        do_repair: bool,
        purge_files: bool,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<()> {
        // Convert path to C string
        let par2_cstr = CString::new(self.par2_file.as_str()).map_err(|e| {
            PostProcessingError::Par2Failed(format!("Invalid PAR2 file path: {}", e))
        })?;

        // Store callback in global storage if provided
        if let Some(callback) = progress_callback {
            *PROGRESS_CALLBACK.lock().unwrap() = Some(callback);
        }

        // Call C API with or without progress callback
        let result = unsafe {
            if PROGRESS_CALLBACK.lock().unwrap().is_some() {
                par2_repair_with_progress(
                    par2_cstr.as_ptr(),
                    do_repair,
                    purge_files,
                    Some(progress_trampoline),
                )
            } else {
                par2_repair_sync(par2_cstr.as_ptr(), do_repair)
            }
        };

        // Clear callback storage
        *PROGRESS_CALLBACK.lock().unwrap() = None;

        // Convert result
        self.convert_result(result, do_repair)
    }

    fn convert_result(&self, result: Par2Result, do_repair: bool) -> Result<()> {
        match result {
            Par2Result::Success => Ok(()),
            Par2Result::RepairPossible => {
                if do_repair {
                    Err(PostProcessingError::Par2Failed(
                        "PAR2 repair possible but not completed".to_string(),
                    )
                    .into())
                } else {
                    Ok(()) // Verification passed, repair is possible if needed
                }
            }
            Par2Result::RepairNotPossible => Err(PostProcessingError::Par2Failed(
                "PAR2 repair not possible: insufficient recovery data".to_string(),
            )
            .into()),
            Par2Result::InvalidArguments => {
                Err(PostProcessingError::Par2Failed("Invalid arguments".to_string()).into())
            }
            Par2Result::InsufficientData => Err(PostProcessingError::Par2Failed(
                "Insufficient critical data in PAR2 files".to_string(),
            )
            .into()),
            Par2Result::RepairFailed => {
                Err(PostProcessingError::Par2Failed("PAR2 repair failed".to_string()).into())
            }
            Par2Result::FileIOError => {
                Err(PostProcessingError::Par2Failed("File I/O error".to_string()).into())
            }
            Par2Result::LogicError => {
                Err(PostProcessingError::Par2Failed("Internal logic error".to_string()).into())
            }
            Par2Result::MemoryError => {
                Err(PostProcessingError::Par2Failed("Out of memory".to_string()).into())
            }
        }
    }
}
