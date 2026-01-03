//! PAR2 verification and repair functionality

use indicatif::ProgressBar;
use std::path::{Path, PathBuf};

use crate::config::PostProcessingConfig;
use crate::error::DlNzbError;
use crate::progress;
use par2_rs::repair::{
    repair_files, FileStatus, ProgressReporter, RecoverySetInfo, RepairResult, VerificationResult,
};
use par2_rs::verify::VerificationConfig;

type Result<T> = std::result::Result<T, DlNzbError>;

/// Result of PAR2 repair attempt
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Par2Status {
    /// No PAR2 files found - safe to proceed with extraction
    NoPar2Files,
    /// PAR2 repair succeeded - files verified/repaired, safe to extract
    Success,
    /// PAR2 repair failed - files may be corrupted, NOT safe to extract
    Failed,
}

/// Bridge between indicatif::ProgressBar and par2_rs::ProgressReporter
struct Par2ProgressReporter {
    pb: ProgressBar,
}

impl Par2ProgressReporter {
    fn new(pb: ProgressBar) -> Self {
        Self { pb }
    }
}

impl ProgressReporter for Par2ProgressReporter {
    fn report_statistics(&self, _recovery_set: &RecoverySetInfo) {}
    fn report_file_opening(&self, _file_name: &str) {}
    fn report_file_status(&self, _file_name: &str, _status: FileStatus) {}
    fn report_scanning(&self, file_name: &str) {
        self.pb.set_message(format!("Scanning: {}", file_name));
        progress::apply_style(&self.pb, progress::ProgressStyle::Par2);
    }
    fn report_scanning_progress(&self, _file_name: &str, bytes_processed: u64, total_bytes: u64) {
        self.pb.set_length(total_bytes);
        self.pb.set_position(bytes_processed);
    }
    fn clear_scanning(&self, _file_name: &str) {}
    fn report_recovery_info(&self, _available: usize, _needed: usize) {}
    fn report_insufficient_recovery(&self, _available: usize, _needed: usize) {}
    fn report_repair_header(&self) {}
    fn report_loading_progress(&self, _files_loaded: usize, _total_files: usize) {}
    fn report_constructing(&self) {
        self.pb.set_message("Constructing repair matrix...");
    }
    fn report_computing_progress(&self, blocks_processed: usize, total_blocks: usize) {
        self.pb.set_message("Repairing...");
        self.pb.set_length(total_blocks as u64);
        self.pb.set_position(blocks_processed as u64);
        progress::apply_style(&self.pb, progress::ProgressStyle::Par2Repair);
    }
    fn report_repair_start(&self, file_name: &str) {
        self.pb.set_message(format!("Repairing: {}", file_name));
    }
    fn report_writing_progress(&self, _file_name: &str, bytes_written: u64, total_bytes: u64) {
        self.pb.set_length(total_bytes);
        self.pb.set_position(bytes_written);
    }
    fn report_repair_complete(&self, _file_name: &str, _repaired: bool) {}
    fn report_repair_failed(&self, _file_name: &str, _error: &str) {}
    fn report_verification_header(&self) {}
    fn report_verification(&self, file_name: &str, _result: VerificationResult) {
        self.pb.set_message(format!("Verified: {}", file_name));
        progress::apply_style(&self.pb, progress::ProgressStyle::Par2Verify);
    }
    fn report_final_result(&self, _result: &RepairResult) {}
}

/// Run PAR2 verification and repair on downloaded files
pub async fn repair_with_par2(
    config: &PostProcessingConfig,
    _download_dir: &Path,
    downloaded_par2_files: &[PathBuf],
    progress_bar: &ProgressBar,
) -> Result<Par2Status> {
    if downloaded_par2_files.is_empty() {
        progress_bar.finish_and_clear();
        return Ok(Par2Status::NoPar2Files);
    }

    // Find the main PAR2 file (index file without .vol)
    // We use the first PAR2 file provided as the entry point
    let main_par2 = downloaded_par2_files.first().ok_or_else(|| {
        DlNzbError::PostProcessing(crate::error::PostProcessingError::NoRarArchives)
    })?; // Fix this later with better error

    let reporter =
        Box::new(Par2ProgressReporter::new(progress_bar.clone())) as Box<dyn ProgressReporter>;
    let verify_config = VerificationConfig::for_repair(0, true);

    // Use spawn_blocking since repair_files is synchronous
    let main_par2_str = main_par2.to_string_lossy().to_string();
    let result =
        tokio::task::spawn_blocking(move || repair_files(&main_par2_str, reporter, &verify_config))
            .await
            .map_err(|e| DlNzbError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    match result {
        Ok((_context, repair_result)) => {
            progress_bar.finish_and_clear();

            match repair_result {
                RepairResult::Success { .. } | RepairResult::NoRepairNeeded { .. } => {
                    // Delete PAR2 files if configured
                    if config.delete_par2_after_repair {
                        for par2_path in downloaded_par2_files {
                            if par2_path.exists() {
                                let _ = std::fs::remove_file(par2_path);
                            }
                        }
                    }
                    println!("  └─ \x1b[33m✓ PAR2 verified\x1b[0m");
                    Ok(Par2Status::Success)
                }
                RepairResult::Failed { message, .. } => {
                    println!("  └─ \x1b[31m✗ PAR2 failed: {}\x1b[0m", message);
                    Ok(Par2Status::Failed)
                }
            }
        }
        Err(e) => {
            println!("  └─ \x1b[31m✗ PAR2 failed: {}\x1b[0m", e);
            Ok(Par2Status::Failed)
        }
    }
}
