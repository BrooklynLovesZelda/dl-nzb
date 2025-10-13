// C API wrapper for par2cmdline-turbo library
// This provides a simplified interface for Rust FFI

#include "../par2cmdline-turbo/src/libpar2.h"
#include <iostream>
#include <sstream>
#include <vector>
#include <string>
#include <cstring>
#include <thread>
#include <regex>
#include <fcntl.h>
#include <unistd.h>

#ifdef __APPLE__
#include <sys/types.h>
#include <sys/sysctl.h>
#include <dirent.h>
#elif defined(__linux__)
// Linux already included unistd.h above
#include <dirent.h>
#elif defined(_WIN32)
#include <windows.h>
#include <io.h>
#endif

// Progress callback type: takes (operation, current, total) and returns nothing
// operation: 0=Scanning, 1=Loading, 2=Verifying, 3=Repairing
typedef void (*ProgressCallback)(uint8_t operation, uint64_t current, uint64_t total);

// Custom stream buffer that captures output and extracts progress
class ProgressStreamBuf : public std::streambuf {
private:
    std::string buffer;
    ProgressCallback callback;

public:
    ProgressStreamBuf(ProgressCallback cb) : callback(cb) {}

protected:
    virtual int_type overflow(int_type c) override {
        if (c != EOF) {
            buffer += static_cast<char>(c);

            // Check for progress lines (e.g., "Scanning: 45.3%\r" or "Loading: 12.5%\r")
            if (c == '\r' || c == '\n') {
                parse_progress();
                buffer.clear();
            }
        }
        return c;
    }

    virtual std::streamsize xsputn(const char* s, std::streamsize count) override {
        buffer.append(s, count);

        // Check if we have a complete line
        size_t pos;
        while ((pos = buffer.find('\r')) != std::string::npos ||
               (pos = buffer.find('\n')) != std::string::npos) {
            parse_progress();
            buffer.erase(0, pos + 1);
        }

        return count;
    }

private:
    void parse_progress() {
        if (!callback) return;

        // Match patterns like "Scanning: 45.3%" or "Loading: 12.5%"
        std::regex progress_regex(R"((Scanning|Loading|Verifying|Repairing):\s*(\d+(?:\.\d+)?)%)");
        std::smatch match;

        if (std::regex_search(buffer, match, progress_regex)) {
            try {
                // Determine operation type
                uint8_t operation = 0;
                std::string op_name = match[1].str();
                if (op_name == "Scanning") operation = 0;
                else if (op_name == "Loading") operation = 1;
                else if (op_name == "Verifying") operation = 2;
                else if (op_name == "Repairing") operation = 3;

                double percent = std::stod(match[2].str());
                // Convert percentage to current/total (0-1000 scale for precision)
                uint64_t current = static_cast<uint64_t>(percent * 10.0);
                uint64_t total = 1000;
                callback(operation, current, total);
            } catch (...) {
                // Ignore parsing errors
            }
        }
    }
};

// C-compatible result enum matching Rust's Par2Result
extern "C" {
    enum Par2Result {
        SUCCESS = 0,
        REPAIR_POSSIBLE = 1,
        REPAIR_NOT_POSSIBLE = 2,
        INVALID_ARGUMENTS = 3,
        INSUFFICIENT_DATA = 4,
        REPAIR_FAILED = 5,
        FILE_IO_ERROR = 6,
        LOGIC_ERROR = 7,
        MEMORY_ERROR = 8,
    };

    // Get system RAM and calculate 1/2 of it for memory limit
    // (matches par2cmdline-turbo default behavior)
    static size_t get_memory_limit() {
        size_t total_memory = 0;

#ifdef __APPLE__
        // macOS: use sysctl
        int mib[2] = {CTL_HW, HW_MEMSIZE};
        size_t length = sizeof(total_memory);
        sysctl(mib, 2, &total_memory, &length, NULL, 0);
#elif defined(__linux__)
        // Linux: use sysconf
        long pages = sysconf(_SC_PHYS_PAGES);
        long page_size = sysconf(_SC_PAGE_SIZE);
        if (pages > 0 && page_size > 0) {
            total_memory = (size_t)pages * (size_t)page_size;
        }
#elif defined(_WIN32)
        // Windows: use GlobalMemoryStatusEx
        MEMORYSTATUSEX status;
        status.dwLength = sizeof(status);
        GlobalMemoryStatusEx(&status);
        total_memory = (size_t)status.ullTotalPhys;
#endif

        // Default to 256MB if we can't detect (matches par2cmdline fallback)
        if (total_memory == 0) {
            total_memory = 256 * 1024 * 1024;
        }

        // Use 1/2 of system RAM (matches par2cmdline-turbo default)
        size_t memory_limit = total_memory / 2;

        // Minimum of 16MB and maximum of 2GB
        const size_t MIN_MEMORY = 16 * 1024 * 1024;         // 16MB minimum
        const size_t MAX_MEMORY = 2048ULL * 1024 * 1024;   // 2GB maximum (32-bit safe)

        if (memory_limit < MIN_MEMORY) memory_limit = MIN_MEMORY;
        if (memory_limit > MAX_MEMORY) memory_limit = MAX_MEMORY;

        return memory_limit;
    }

    // Get optimal thread count (matches par2cmdline-turbo behavior)
    static unsigned int get_thread_count() {
        unsigned int hw_threads = std::thread::hardware_concurrency();
        // hardware_concurrency() returns 0 if unable to detect
        return (hw_threads > 0) ? hw_threads : 2; // Fallback to 2 threads
    }

    // Simplified synchronous repair function for Rust FFI with progress callback
    Par2Result par2_repair_with_progress(
        const char* parfilename,
        bool do_repair,
        bool purge_files,
        ProgressCallback progress_callback
    ) {
        if (!parfilename) {
            return INVALID_ARGUMENTS;
        }

        std::string par2file(parfilename);

        // Extract directory from par2 file path
        std::string basepath;
        size_t last_slash = par2file.find_last_of("/\\");
        if (last_slash != std::string::npos) {
            basepath = par2file.substr(0, last_slash + 1);
        } else {
            basepath = "./";
        }

        // Collect all non-PAR2 files in the directory to scan for misnamed files
        // This is critical for obfuscated Usenet downloads where filenames don't match
        std::vector<std::string> extrafiles;

#ifndef _WIN32
        DIR *dir = opendir(basepath.c_str());
        if (dir) {
            struct dirent *entry;
            while ((entry = readdir(dir)) != nullptr) {
                std::string filename = entry->d_name;
                // Skip . and .. and PAR2 files
                if (filename != "." && filename != ".." &&
                    filename.find(".par2") == std::string::npos &&
                    filename.find(".PAR2") == std::string::npos &&
                    filename != ".DS_Store") {  // Skip macOS metadata
                    // Use full path for extrafiles
                    extrafiles.push_back(basepath + filename);
                }
            }
            closedir(dir);
        }
#else
        // Windows directory scanning
        WIN32_FIND_DATAA find_data;
        HANDLE hFind = FindFirstFileA((basepath + "*").c_str(), &find_data);
        if (hFind != INVALID_HANDLE_VALUE) {
            do {
                std::string filename = find_data.cFileName;
                if (filename != "." && filename != ".." &&
                    filename.find(".par2") == std::string::npos &&
                    filename.find(".PAR2") == std::string::npos &&
                    !(find_data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)) {
                    // Use full path for extrafiles
                    extrafiles.push_back(basepath + filename);
                }
            } while (FindNextFileA(hFind, &find_data) != 0);
            FindClose(hFind);
        }
#endif

        // Get adaptive parameters (matches par2cmdline-turbo defaults)
        size_t memory_limit = get_memory_limit();     // 1/2 system RAM
        unsigned int nthreads = get_thread_count();   // Auto-detect CPU cores

        // Create progress stream buffer if callback provided
        ProgressStreamBuf progress_buf(progress_callback);
        std::ostream progress_stream(&progress_buf);

        // Create error stream that discards output
        std::ostringstream null_err;

        // Call par2repair with proper parameters
        // CRITICAL: memorylimit must NOT be 0!
        // extrafiles contains all non-PAR2 files in directory for hash-based matching
        // Progress requires nlNormal (nlQuiet suppresses progress output)
        Result result = par2repair(
            progress_callback ? progress_stream : null_err,  // stdout (captured or discarded)
            null_err,                       // stderr (discarded)
            progress_callback ? nlNormal : nlSilent,  // noise level (normal for progress, silent otherwise)
            memory_limit,                   // memory limit (1/2 system RAM, 16MB-2GB)
            basepath,                       // basepath
            nthreads,                       // nthreads (auto-detected)
            2,                              // filethreads (matches _FILE_THREADS default)
            par2file,                       // PAR2 file path
            extrafiles,                     // extra files to scan for hash matches (misnamed files)
            do_repair,                      // do repair
            purge_files,                    // purge files (delete PAR2 files after successful repair)
            false,                          // skip data
            0                               // skip leaway
        );

        // Convert Result to Par2Result
        switch (result) {
            case eSuccess:
                return SUCCESS;
            case eRepairPossible:
                return REPAIR_POSSIBLE;
            case eRepairNotPossible:
                return REPAIR_NOT_POSSIBLE;
            case eInvalidCommandLineArguments:
                return INVALID_ARGUMENTS;
            case eInsufficientCriticalData:
                return INSUFFICIENT_DATA;
            case eRepairFailed:
                return REPAIR_FAILED;
            case eFileIOError:
                return FILE_IO_ERROR;
            case eLogicError:
                return LOGIC_ERROR;
            case eMemoryError:
                return MEMORY_ERROR;
            default:
                return LOGIC_ERROR;
        }
    }

    // Backward-compatible function without progress callback
    Par2Result par2_repair_sync(
        const char* parfilename,
        bool do_repair
    ) {
        return par2_repair_with_progress(parfilename, do_repair, false, nullptr);
    }
}
