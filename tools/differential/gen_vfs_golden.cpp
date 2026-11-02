// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector generator for the rzh-vfs port.
//
// Like gen_compress_golden.cpp, this driver avoids a second
// transcription where one already exists: it links the REAL EAC RefPack
// codec sources (Core/Libraries/Source/Compression/EAC/) for every
// compressed payload. The archive and overlay logic itself cannot be
// compiled standalone (BIGFileSystem.cpp pulls in the whole game
// engine), so the tree, overlay and routing logic is transcribed here
// verbatim from the baseline sources, minus engine plumbing (memory
// pools, logging, audio, registry):
//
//   Core/GameEngine/Source/Common/System/FileSystem.cpp        (path keys, routing)
//   Core/GameEngine/Source/Common/System/BIGFileSystem.cpp     (BIGF parse, mounting)
//   Core/GameEngine/Source/Common/System/ArchiveFile.cpp       (per-archive tree)
//   Core/GameEngine/Source/Common/System/ArchiveFileSystem.cpp (overlay, instances)
//   Core/GameEngine/Source/Common/System/RAMFile.cpp           (whole-entry reads)
//   Core/GameEngineDevice/Source/Win32Device/Common/Win32LocalFileSystem.cpp
//     (listing semantics, on std::filesystem)
//   Core/Libraries/Source/Compression/CompressionManager.cpp   (header dispatch)
//   GeneralsMD/Code/GameEngine/Source/Common/System/DataChunk.cpp
//     (CachedFileInputStream::open: the decompress-on-read consumer flow)
//   Core/GameEngine/Include/Common/STLTypedefs.h + Dependencies/Utility/Utility/STLUtils.h
//     (FilenameLess set, get_range instance walk)
//
// The Rust port must replay these commands with identical outputs
// (parity level P0).
//
// Platform notes for regeneration:
//   - Vector paths never start with two separators: comparePath's UNC
//     pair rule is Windows-only in both the C and the port, and would
//     produce platform-dependent orderings.
//   - Local file queries use exact-case paths (the local layer's case
//     sensitivity follows the platform, like _access/fopen in the C).
//   - Local file names never collide case-insensitively.
//
// String arguments and results use '-' for the empty string (no test
// path, pattern or key is literally "-").
//
// Usage (wired up by tools/differential/regenerate.sh):
//   gen_vfs_golden <out.txt> <scratch-dir>
//
// Output format (whitespace-separated; '-' is the empty byte string):
//   pathkey  <path> <key|FAIL>
//   cmp      <a> <b> <int>
//   wild     <subject> <pattern> <0|1>
//   bigdef   <archive-path> <archive-hex>          (also written to scratch dir)
//   localdef <path> <content-hex>                  (created under scratch dir)
//   mountdir <dir> <mask> <overwrite> <added>      (loadArchiveFilesFromDirectory)
//   exists   <path> <instance> <0|1>
//   open     <path> <instance> <hex|->             (whole-file read, raw bytes)
//   decompof <path> <instance> <hex|->             (CachedFileInputStream flow)
//   info     <path> <instance> <size|->            (sizes only; timestamps are machine state)
//   list     <dir> <pattern> <recursive> <count> <path> ...
//   comptype <hex> <name>
//   uncsize  <hex> <dec>
//   decomp   <hex> <hex|->                         (EAR dispatch; FAIL not expected)

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iterator>
#include <map>
#include <set>
#include <string>
#include <vector>

#include <dirent.h>
#include <sys/stat.h>

// The real EAC codec sources, linked as separate translation units
// exactly like the game build (declarations mirror refcodex.h without
// its default arguments; the header cannot be included twice).
extern "C" {
int REF_decode(void *dest, const void *compresseddata, int *compressedsize);
int REF_encode(void *compresseddata, const void *source, int sourcesize, int *opts);
}

// ============================================================================
// Deterministic content (LCG, same shape as gen_compress_golden.cpp)
// ============================================================================

static uint32_t lcg_state = 0xcafebeefu;
static uint32_t lcg(void) {
    lcg_state = lcg_state * 1664525u + 1013904223u;
    return lcg_state;
}

static std::vector<uint8_t> lcg_bytes(size_t n, uint32_t step) {
    std::vector<uint8_t> out(n);
    for (size_t i = 0; i < n; ++i) {
        out[i] = static_cast<uint8_t>((lcg() >> (i % step * 3)) ^ (lcg() & 0xff));
    }
    return out;
}

// Deterministic compressible text (like INI data)
static std::vector<uint8_t> ini_like(size_t lines) {
    std::vector<uint8_t> out;
    for (size_t i = 0; i < lines; ++i) {
        std::string line = "; comment line number " + std::to_string(i % 37) +
                           " with repeated repeated repeated filler text\r\n";
        out.insert(out.end(), line.begin(), line.end());
    }
    return out;
}

// The empty-string placeholder for path/pattern/key arguments.
static const char *arg(const std::string &s) { return s.empty() ? "-" : s.c_str(); }

static std::string hex(const std::vector<uint8_t> &bytes) {
    static const char *digits = "0123456789abcdef";
    std::string out;
    out.reserve(bytes.size() * 2);
    for (uint8_t b : bytes) {
        out.push_back(digits[b >> 4]);
        out.push_back(digits[b & 15]);
    }
    if (out.empty())
        out.push_back('-');
    return out;
}

// ============================================================================
// Path utilities — transcribed from FileSystem.cpp / PathUtil.h
// ============================================================================

#define MAX_PATH_260 260

static bool isPathSeparator(char ch) { return ch == '/' || ch == '\\'; }

static const char *getPathKeyPairEnd(const char *path) {
#ifdef _WIN32
    if (isPathSeparator(path[0]) && isPathSeparator(path[1]))
        return path + 2;
#endif
    return path;
}

static unsigned char toPathKeyChar(char ch) {
    if (isPathSeparator(ch))
        return '\\';
    if (ch >= 'A' && ch <= 'Z')
        return static_cast<unsigned char>(ch + ('a' - 'A'));
    return static_cast<unsigned char>(ch);
}

static bool makePathKey(char *key, size_t keySize, const char *path) {
    char *const keyBegin = key;
    const char *const keyEnd = key + keySize - 1;
    const char *const pairEnd = getPathKeyPairEnd(path);

    for (;;) {
        const unsigned char ch = toPathKeyChar(*path);

        if (ch == '\0')
            break;

        if (key == keyEnd) {
            *keyBegin = '\0';
            return false;
        }

        *key++ = static_cast<char>(ch);
        ++path;

        if (ch == '\\') {
            while (path > pairEnd && isPathSeparator(*path))
                ++path;
        }
    }

    *key = '\0';
    return true;
}

static int comparePath(const char *a, const char *b) {
    const char *const pairEndA = getPathKeyPairEnd(a);
    const char *const pairEndB = getPathKeyPairEnd(b);

    for (;;) {
        const unsigned char ca = toPathKeyChar(*a);
        const unsigned char cb = toPathKeyChar(*b);

        if (ca != cb || ca == '\0')
            return static_cast<int>(ca) - static_cast<int>(cb);

        ++a;
        ++b;

        if (ca == '\\') {
            while (a > pairEndA && isPathSeparator(*a))
                ++a;
            while (b > pairEndB && isPathSeparator(*b))
                ++b;
        }
    }
}

struct FilenameLess {
    bool operator()(const std::string &a, const std::string &b) const {
        return comparePath(a.c_str(), b.c_str()) < 0;
    }
};

typedef std::set<std::string, FilenameLess> FilenameList;

static const char *getFileNamePtr(const char *path) {
    const char *lastSeparator = path;
    const char *scan = path;
    while (*scan) {
        if (isPathSeparator(*scan))
            lastSeparator = scan;
        ++scan;
    }
    return isPathSeparator(*lastSeparator) ? lastSeparator + 1 : path;
}

// AsciiString::nextToken(&tok, "\\/") over std::string
static bool nextToken(std::string &tokenizer, std::string &token) {
    if (tokenizer.empty()) {
        token.clear();
        return false;
    }
    size_t start = 0;
    while (start < tokenizer.size() && isPathSeparator(tokenizer[start]))
        ++start;
    size_t end = start;
    while (end < tokenizer.size() && !isPathSeparator(tokenizer[end]))
        ++end;
    if (end == start) {
        tokenizer.clear();
        token.clear();
        return false;
    }
    token = tokenizer.substr(start, end - start);
    tokenizer = tokenizer.substr(end);
    return true;
}

// ============================================================================
// SearchStringMatches — transcribed from ArchiveFile.cpp (recursive)
// ============================================================================

static bool SearchStringMatches(const char *str, const char *searchString) {
    if (*str == '\0') {
        return *searchString == '\0';
    }
    if (*searchString == '\0') {
        return false;
    }

    const char *c1 = str;
    const char *c2 = searchString;

    while ((*c1 == *c2) || (*c2 == '?') || (*c2 == '*')) {
        if ((*c1 == *c2) || (*c2 == '?')) {
            ++c1;
            ++c2;
        } else if (*c2 == '*') {
            ++c2;
            if (*c2 == 0) {
                return true;
            }
            while (*c1 != 0) {
                if (SearchStringMatches(c1, c2)) {
                    return true;
                }
                ++c1;
            }
        }
        if (*c1 == 0) {
            if (*c2 == 0) {
                return true;
            }
            return false;
        }
        if (*c2 == 0) {
            return false;
        }
    }
    return false;
}

// ============================================================================
// ArchiveFile — transcribed from ArchiveFile.cpp / BIGFileSystem.cpp
// ============================================================================

struct ArchivedFileInfo {
    std::string m_filename;
    std::string m_archiveFilename;
    uint32_t m_offset = 0;
    uint32_t m_size = 0;
};

struct DetailedArchivedDirectoryInfo {
    std::map<std::string, DetailedArchivedDirectoryInfo> m_directories;
    std::map<std::string, ArchivedFileInfo> m_files;
};

struct ArchiveFile {
    std::string m_name;
    DetailedArchivedDirectoryInfo m_rootDirectory;
    const std::vector<uint8_t> *m_data = nullptr; // the still-open backing

    // ArchiveFile::addFile (token loop on the token)
    void addFile(const std::string &path, const ArchivedFileInfo *fileInfo) {
        DetailedArchivedDirectoryInfo *dirInfo = &m_rootDirectory;

        std::string tokenizer = path;
        std::transform(tokenizer.begin(), tokenizer.end(), tokenizer.begin(), ::tolower);
        std::string token;
        nextToken(tokenizer, token);

        while (!token.empty()) {
            dirInfo = &dirInfo->m_directories[token];
            nextToken(tokenizer, token);
        }

        dirInfo->m_files[fileInfo->m_filename] = *fileInfo;
    }

    // ArchiveFile::getArchivedFileInfo (tokenizer-remainder loop)
    const ArchivedFileInfo *getArchivedFileInfo(const std::string &filename) const {
        const DetailedArchivedDirectoryInfo *dirInfo = &m_rootDirectory;

        std::string tokenizer = filename;
        std::transform(tokenizer.begin(), tokenizer.end(), tokenizer.begin(), ::tolower);
        std::string token;
        nextToken(tokenizer, token);

        while (!tokenizer.empty()) {
            auto it = dirInfo->m_directories.find(token);
            if (it != dirInfo->m_directories.end()) {
                dirInfo = &it->second;
                nextToken(tokenizer, token);
            } else {
                return nullptr;
            }
        }

        auto it = dirInfo->m_files.find(token);
        return it != dirInfo->m_files.end() ? &it->second : nullptr;
    }

    // ArchiveFile::getFileListInDirectory (public wrapper + private
    // recursion: directories first, then matching files, dedup by set)
    void getFileListInDirectory(const std::string &originalDirectory,
                                const std::string &searchName,
                                FilenameList &filenameList) const {
        const DetailedArchivedDirectoryInfo *dirInfo = &m_rootDirectory;

        std::string tokenizer = originalDirectory;
        std::transform(tokenizer.begin(), tokenizer.end(), tokenizer.begin(), ::tolower);
        std::string token;
        nextToken(tokenizer, token);

        while (!token.empty()) {
            auto it = dirInfo->m_directories.find(token);
            if (it == dirInfo->m_directories.end())
                return;
            dirInfo = &it->second;
            nextToken(tokenizer, token);
        }

        listRecursive(dirInfo, originalDirectory, searchName, filenameList);
    }

    void listRecursive(const DetailedArchivedDirectoryInfo *dirInfo,
                       const std::string &currentDirectory,
                       const std::string &searchName,
                       FilenameList &filenameList) const {
        for (const auto &dir : dirInfo->m_directories) {
            std::string tempDirName = currentDirectory;
            if (!tempDirName.empty() && tempDirName.back() != '\\')
                tempDirName.push_back('\\');
            tempDirName += dir.first;
            listRecursive(&dir.second, tempDirName, searchName, filenameList);
        }
        for (const auto &file : dirInfo->m_files) {
            if (SearchStringMatches(file.second.m_filename.c_str(), searchName.c_str())) {
                std::string tempfilename = currentDirectory;
                if (!tempfilename.empty() && tempfilename.back() != '\\')
                    tempfilename.push_back('\\');
                tempfilename += file.second.m_filename;
                filenameList.insert(tempfilename);
            }
        }
    }
};

// BIGFileSystem::openArchiveFile — the parse, over in-memory bytes
// (the read(fp, n) short-count semantics are transcribed with bounds)
static ArchiveFile *openArchiveFileBytes(const std::string &filename,
                                         const std::vector<uint8_t> &data) {
    size_t pos = 0;
    auto read = [&](void *dst, size_t n) -> size_t {
        size_t avail = data.size() > pos ? data.size() - pos : 0;
        size_t got = n < avail ? n : avail;
        memcpy(dst, data.data() + pos, got);
        pos += got;
        return got;
    };

    struct Header {
        char identifier[4];
        uint8_t archiveFileSize[4]; // little endian
        uint8_t numFiles[4];        // big endian
        uint8_t firstEntryOffset[4];
    } header;

    if (read(&header, sizeof(header)) != sizeof(header) ||
        memcmp(header.identifier, "BIGF", 4) != 0) {
        return nullptr;
    }

    uint32_t numLittleFiles = (uint32_t(header.numFiles[0]) << 24) |
                              (uint32_t(header.numFiles[1]) << 16) |
                              (uint32_t(header.numFiles[2]) << 8) |
                              uint32_t(header.numFiles[3]);

    ArchiveFile *archiveFile = new ArchiveFile();
    archiveFile->m_name = filename;
    archiveFile->m_data = &data;

    std::string archiveFileName = filename;
    std::transform(archiveFileName.begin(), archiveFileName.end(),
                   archiveFileName.begin(), ::tolower);

    ArchivedFileInfo fileInfo;
    bool corrupt = false;
    for (uint32_t i = 0; i < numLittleFiles && !corrupt; ++i) {
        uint8_t entry[8];
        if (read(entry, sizeof(entry)) != sizeof(entry)) {
            corrupt = true;
            break;
        }

        fileInfo.m_archiveFilename = archiveFileName;
        fileInfo.m_offset = (uint32_t(entry[0]) << 24) | (uint32_t(entry[1]) << 16) |
                            (uint32_t(entry[2]) << 8) | uint32_t(entry[3]);
        fileInfo.m_size = (uint32_t(entry[4]) << 24) | (uint32_t(entry[5]) << 16) |
                          (uint32_t(entry[6]) << 8) | uint32_t(entry[7]);

        // readNullTerminatedString(fp, buffer, sizeof(buffer))
        std::string pathName;
        bool terminated = false;
        for (int b = 0; b < MAX_PATH_260; ++b) {
            uint8_t one;
            if (read(&one, 1) != 1) {
                delete archiveFile;
                return nullptr; // -1
            }
            if (one == 0) {
                terminated = true;
                break;
            }
            pathName.push_back(char(one));
        }
        if (!terminated) {
            corrupt = true;
            break;
        }

        const char *leafName = getFileNamePtr(pathName.c_str());
        fileInfo.m_filename = leafName;
        std::transform(fileInfo.m_filename.begin(), fileInfo.m_filename.end(),
                       fileInfo.m_filename.begin(), ::tolower);
        std::string dirPart = pathName.substr(0, size_t(leafName - pathName.c_str()));

        archiveFile->addFile(dirPart, &fileInfo);
    }

    if (corrupt) {
        return nullptr;
    }
    return archiveFile;
}

// ============================================================================
// ArchiveFileSystem — transcribed from ArchiveFileSystem.cpp
// ============================================================================

typedef std::multimap<std::string, ArchiveFile *> ArchivedFileLocationMap;

struct ArchivedDirectoryInfo {
    std::string m_path;
    std::string m_directoryName;
    std::map<std::string, ArchivedDirectoryInfo> m_directories;
    ArchivedFileLocationMap m_files;
};

// stl::advance_in_range (STLUtils.h)
template <typename Iter>
static Iter advance_in_range(Iter first, Iter last, ptrdiff_t n) {
    if (n <= 0)
        return first;
    const ptrdiff_t count = std::distance(first, last);
    if (n >= count)
        return last;
    std::advance(first, n);
    return first;
}

struct ArchiveFileSystem {
    std::map<std::string, ArchiveFile *> m_archiveFileMap;
    ArchivedDirectoryInfo m_rootDirectory;
    std::vector<ArchiveFile *> m_owned; // keeps the parsed archives alive

    struct DirectoryResult {
        ArchivedDirectoryInfo *dirInfo = nullptr;
        std::string lastToken;
        bool valid() const { return dirInfo != nullptr; }
    };

    // ArchiveFileSystem::getArchivedDirectoryInfo
    DirectoryResult getArchivedDirectoryInfo(const std::string &directory) {
        DirectoryResult result;
        ArchivedDirectoryInfo *dirInfo = &m_rootDirectory;

        std::string tokenizer = directory;
        std::transform(tokenizer.begin(), tokenizer.end(), tokenizer.begin(), ::tolower);
        std::string token;
        nextToken(tokenizer, token);

        while (!tokenizer.empty()) {
            auto tempiter = dirInfo->m_directories.find(token);
            if (tempiter != dirInfo->m_directories.end()) {
                dirInfo = &tempiter->second;
                nextToken(tokenizer, token);
            } else {
                result.dirInfo = nullptr;
                result.lastToken.clear();
                return result;
            }
        }

        result.dirInfo = dirInfo;
        result.lastToken = token;
        return result;
    }

    // ArchiveFileSystem::loadIntoDirectoryTree (multimap insert-hint
    // semantics: overwrite inserts before the first equal element)
    void loadIntoDirectoryTree(ArchiveFile *archiveFile, bool overwrite) {
        FilenameList filenameList;
        archiveFile->getFileListInDirectory("", "*", filenameList);

        for (const auto &path : filenameList) {
            ArchivedDirectoryInfo *dirInfo = &m_rootDirectory;

            std::string tokenizer = path; // listing paths are lowercase already
            std::string token;
            nextToken(tokenizer, token);

            std::string builtPath;
            while (!tokenizer.empty()) {
                builtPath += token;
                builtPath.push_back('\\');

                auto tempiter = dirInfo->m_directories.find(token);
                if (tempiter == dirInfo->m_directories.end()) {
                    dirInfo = &dirInfo->m_directories[token];
                    dirInfo->m_path = builtPath;
                    dirInfo->m_directoryName = token;
                } else {
                    dirInfo = &tempiter->second;
                }

                nextToken(tokenizer, token);
            }

            ArchivedFileLocationMap::iterator fileIt;
            if (overwrite) {
                fileIt = dirInfo->m_files.find(token);
            } else {
                fileIt = dirInfo->m_files.end();
            }
            dirInfo->m_files.insert(fileIt, std::make_pair(token, archiveFile));
        }
    }

    bool doesFileExist(const std::string &filename, uint8_t instance) const {
        DirectoryResult result = const_cast<ArchiveFileSystem *>(this)
                                     ->getArchivedDirectoryInfo(filename);
        if (!result.valid())
            return false;
        auto range = std::make_pair(result.dirInfo->m_files.lower_bound(result.lastToken),
                                    result.dirInfo->m_files.upper_bound(result.lastToken));
        auto it = advance_in_range(range.first, range.second, instance);
        return it != range.second;
    }

    ArchiveFile *getArchiveFile(const std::string &filename, uint8_t instance) const {
        DirectoryResult result = const_cast<ArchiveFileSystem *>(this)
                                     ->getArchivedDirectoryInfo(filename);
        if (!result.valid())
            return nullptr;
        auto range = std::make_pair(result.dirInfo->m_files.lower_bound(result.lastToken),
                                    result.dirInfo->m_files.upper_bound(result.lastToken));
        auto it = advance_in_range(range.first, range.second, instance);
        return it != range.second ? it->second : nullptr;
    }

    // ArchiveFileSystem::openFile → ArchiveFile::openFile → RAMFile
    // (READ path: whole-entry read from the backing)
    bool readFile(const std::string &filename, uint8_t instance,
                  std::vector<uint8_t> &out) const {
        ArchiveFile *archive = getArchiveFile(filename, instance);
        if (!archive)
            return false;
        const ArchivedFileInfo *info = archive->getArchivedFileInfo(filename);
        if (!info)
            return false;
        if (uint64_t(info->m_offset) + info->m_size > archive->m_data->size())
            return false; // RAMFile::openFromArchive short read
        out.assign(archive->m_data->begin() + long(info->m_offset),
                   archive->m_data->begin() + long(info->m_offset) + long(info->m_size));
        return true;
    }

    void getFileListInDirectory(const std::string &originalDirectory,
                                const std::string &searchName,
                                FilenameList &filenameList) const {
        for (const auto &it : m_archiveFileMap) {
            it.second->getFileListInDirectory(originalDirectory, searchName, filenameList);
        }
    }
};

typedef std::multimap<std::string, ArchiveFile *> ArchivedFileLocationMap;

// ============================================================================
// LocalFileSystem — Win32LocalFileSystem semantics on std::filesystem
// ============================================================================

struct LocalFileSystem {
    std::string m_root;

    bool doesFileExist(const std::string &filename) const {
        std::error_code ec;
        return std::filesystem::exists(join(filename), ec);
    }

    bool readFile(const std::string &filename, std::vector<uint8_t> &out) const {
        std::error_code ec;
        auto size = std::filesystem::file_size(join(filename), ec);
        if (ec)
            return false;
        std::ifstream f(join(filename), std::ios::binary);
        if (!f)
            return false;
        out.resize(size_t(size));
        f.read(reinterpret_cast<char *>(out.data()), std::streamsize(size));
        return bool(f);
    }

    uint64_t fileSize(const std::string &filename) const {
        std::error_code ec;
        auto size = std::filesystem::file_size(join(filename), ec);
        return ec ? ~0ull : size;
    }

    // Win32LocalFileSystem::getFileListInDirectory: match the pattern in
    // the directory (case-insensitively, like FindFirstFile), then
    // recurse into subdirectories when searchSubdirectories.
    void getFileListInDirectory(const std::string &currentDirectory,
                                const std::string &originalDirectory,
                                const std::string &searchName,
                                FilenameList &filenameList,
                                bool searchSubdirectories) const {
        std::string directory = originalDirectory + currentDirectory;
        std::string dirPath = join(directory);

        if (DIR *dir = opendir(dirPath.c_str())) {
            std::string pattern = searchName;
            std::transform(pattern.begin(), pattern.end(), pattern.begin(), ::tolower);
            while (dirent *entry = readdir(dir)) {
                std::string name = entry->d_name;
                if (name == "." || name == "..")
                    continue;
                std::string full = dirPath + "/" + name;
                struct stat st {};
                if (stat(full.c_str(), &st) != 0)
                    continue;
                if (S_ISDIR(st.st_mode))
                    continue; // files only in the pattern pass
                std::string lowered = name;
                std::transform(lowered.begin(), lowered.end(), lowered.begin(), ::tolower);
                if (SearchStringMatches(lowered.c_str(), pattern.c_str())) {
                    filenameList.insert(directory + name);
                }
            }
            closedir(dir);
        }

        if (searchSubdirectories) {
            if (DIR *dir = opendir(dirPath.c_str())) {
                while (dirent *entry = readdir(dir)) {
                    std::string name = entry->d_name;
                    if (name == "." || name == "..")
                        continue;
                    std::string full = dirPath + "/" + name;
                    struct stat st {};
                    if (stat(full.c_str(), &st) != 0 || !S_ISDIR(st.st_mode))
                        continue;
                    std::string tempsearchstr = currentDirectory + name + "\\";
                    getFileListInDirectory(tempsearchstr, originalDirectory, searchName,
                                           filenameList, searchSubdirectories);
                }
                closedir(dir);
            }
        }
    }

    std::string join(const std::string &gamePath) const {
        std::string native = gamePath;
        for (char &c : native)
            if (c == '\\')
                c = '/';
        return m_root + "/" + native;
    }
};

// ============================================================================
// FileSystem — the overlay routing, transcribed from FileSystem.cpp
// ============================================================================

struct FileSystem {
    LocalFileSystem m_local;
    ArchiveFileSystem m_archive;

    // FileSystem::openFile (read path; archive opens never carry the
    // WRITE copy-out flow in this driver)
    bool openFile(const std::string &filename, uint8_t instance,
                  std::vector<uint8_t> &out) const {
        uint8_t archiveInstance = instance;
        if (instance != 0) {
            if (m_local.doesFileExist(filename))
                --archiveInstance;
        } else {
            if (m_local.readFile(filename, out))
                return true;
        }
        return m_archive.readFile(filename, archiveInstance, out);
    }

    // FileSystem::doesFileExist
    bool doesFileExist(const std::string &filename, uint8_t instance) const {
        uint8_t archiveInstance = instance;
        if (m_local.doesFileExist(filename)) {
            if (instance == 0)
                return true;
            --archiveInstance;
        }
        return m_archive.doesFileExist(filename, archiveInstance);
    }

    // FileSystem::getFileInfo (sizes only; the C also copies timestamps)
    bool getFileInfo(const std::string &filename, uint8_t instance, uint64_t &size) const {
        uint8_t archiveInstance = instance;
        if (m_local.doesFileExist(filename)) {
            if (instance == 0) {
                size = m_local.fileSize(filename);
                return true;
            }
            --archiveInstance;
        }
        ArchiveFile *archive = m_archive.getArchiveFile(filename, archiveInstance);
        if (!archive)
            return false;
        const ArchivedFileInfo *info = archive->getArchivedFileInfo(filename);
        if (!info)
            return false;
        size = info->m_size;
        return true;
    }

    void getFileListInDirectory(const std::string &directory, const std::string &searchName,
                                bool searchSubdirectories, FilenameList &out) const {
        m_local.getFileListInDirectory("", directory, searchName, out, searchSubdirectories);
        m_archive.getFileListInDirectory(directory, searchName, out);
    }
};

// ============================================================================
// CompressionManager — transcribed (RefPack wired to the REAL codec)
// ============================================================================

enum CompressionType {
    COMPRESSION_NONE, COMPRESSION_REFPACK, COMPRESSION_NOXLZH, COMPRESSION_ZLIB1,
    COMPRESSION_ZLIB2, COMPRESSION_ZLIB3, COMPRESSION_ZLIB4, COMPRESSION_ZLIB5,
    COMPRESSION_ZLIB6, COMPRESSION_ZLIB7, COMPRESSION_ZLIB8, COMPRESSION_ZLIB9,
    COMPRESSION_BTREE, COMPRESSION_HUFF,
};

static const char *compressionName(CompressionType t) {
    switch (t) {
        case COMPRESSION_NONE: return "none";
        case COMPRESSION_REFPACK: return "refpack";
        case COMPRESSION_NOXLZH: return "noxlzh";
        case COMPRESSION_ZLIB1: return "zlib1";
        case COMPRESSION_ZLIB2: return "zlib2";
        case COMPRESSION_ZLIB3: return "zlib3";
        case COMPRESSION_ZLIB4: return "zlib4";
        case COMPRESSION_ZLIB5: return "zlib5";
        case COMPRESSION_ZLIB6: return "zlib6";
        case COMPRESSION_ZLIB7: return "zlib7";
        case COMPRESSION_ZLIB8: return "zlib8";
        case COMPRESSION_ZLIB9: return "zlib9";
        case COMPRESSION_BTREE: return "btree";
        case COMPRESSION_HUFF: return "huff";
    }
    return "?";
}

static CompressionType getCompressionType(const void *mem, int len) {
    if (len < 8)
        return COMPRESSION_NONE;
    const char *m = static_cast<const char *>(mem);
    if (memcmp(m, "NOX\0", 4) == 0) return COMPRESSION_NOXLZH;
    if (memcmp(m, "ZL1\0", 4) == 0) return COMPRESSION_ZLIB1;
    if (memcmp(m, "ZL2\0", 4) == 0) return COMPRESSION_ZLIB2;
    if (memcmp(m, "ZL3\0", 4) == 0) return COMPRESSION_ZLIB3;
    if (memcmp(m, "ZL4\0", 4) == 0) return COMPRESSION_ZLIB4;
    if (memcmp(m, "ZL5\0", 4) == 0) return COMPRESSION_ZLIB5;
    if (memcmp(m, "ZL6\0", 4) == 0) return COMPRESSION_ZLIB6;
    if (memcmp(m, "ZL7\0", 4) == 0) return COMPRESSION_ZLIB7;
    if (memcmp(m, "ZL8\0", 4) == 0) return COMPRESSION_ZLIB8;
    if (memcmp(m, "ZL9\0", 4) == 0) return COMPRESSION_ZLIB9;
    if (memcmp(m, "EAB\0", 4) == 0) return COMPRESSION_BTREE;
    if (memcmp(m, "EAH\0", 4) == 0) return COMPRESSION_HUFF;
    if (memcmp(m, "EAR\0", 4) == 0) return COMPRESSION_REFPACK;
    return COMPRESSION_NONE;
}

static int getUncompressedSize(const void *mem, int len) {
    if (len < 8)
        return len;
    if (getCompressionType(mem, len) == COMPRESSION_NONE)
        return len;
    // *(Int*)(mem+4): host order of the original Windows build (LE)
    const uint8_t *m = static_cast<const uint8_t *>(mem);
    return int(uint32_t(m[4]) | (uint32_t(m[5]) << 8) | (uint32_t(m[6]) << 16) |
               (uint32_t(m[7]) << 24));
}

static int decompressData(const void *srcVoid, int srcLen, void *destVoid, int destLen) {
    if (srcLen < 8)
        return 0;
    const uint8_t *src = static_cast<const uint8_t *>(srcVoid);
    uint8_t *dest = static_cast<uint8_t *>(destVoid);
    CompressionType compType = getCompressionType(src, srcLen);
    if (compType == COMPRESSION_REFPACK) {
        int slen = srcLen - 8;
        int ret = REF_decode(dest, src + 8, &slen);
        if (ret)
            return ret;
        return 0;
    }
    (void)destLen;
    (void)dest;
    return 0; // only RefPack is wired in this driver (see header note)
}

// CompressionManager::compressData(COMPRESSION_REFPACK)
static std::vector<uint8_t> compressRefpack(const std::vector<uint8_t> &payload) {
    std::vector<uint8_t> out(8 + payload.size() + payload.size() / 2 + 1024, 0);
    memcpy(out.data(), "EAR\0", 4);
    int opts = 0;
    int ret = REF_encode(out.data() + 8, payload.data(), int(payload.size()), &opts);
    if (ret == 0 && !payload.empty())
        return {}; // encoder refused (cannot happen for these inputs)
    uint32_t srcLen = uint32_t(payload.size());
    memcpy(out.data() + 4, &srcLen, 4); // little endian host order
    out.resize(8 + size_t(ret));
    return out;
}

// CachedFileInputStream::open (DataChunk.cpp): the decompress-on-read
// consumer flow. The m_size == 0 case never reads a buffer and returns
// FALSE — represented by the caller as "-".
static bool cachedInputStreamRead(const std::vector<uint8_t> &raw,
                                  std::vector<uint8_t> &served) {
    served = raw;
    if (raw.empty())
        return false; // if (m_size) { ... } — empty files never open
    if (getCompressionType(raw.data(), int(raw.size())) == COMPRESSION_NONE) {
        return true; // not compressed: the bytes are used as they are
    }
    int uncompLen = getUncompressedSize(raw.data(), int(raw.size()));
    std::vector<uint8_t> uncomp(size_t(uncompLen < 0 ? 0 : uncompLen));
    int actualLen = decompressData(raw.data(), int(raw.size()), uncomp.data(), uncompLen);
    if (actualLen == uncompLen) {
        served = uncomp;
    }
    return true; // "Decompression failed - using compressed data" keeps raw
}

// ============================================================================
// BIG image builder (the format the shipped archives use)
// ============================================================================

static std::vector<uint8_t> buildBig(const std::vector<std::pair<std::string, std::vector<uint8_t>>> &entries) {
    size_t tableLen = 0;
    for (const auto &e : entries)
        tableLen += 8 + e.first.size() + 1;
    uint32_t at = uint32_t(16 + tableLen);
    std::vector<uint8_t> out;
    out.reserve(size_t(at));
    out.insert(out.end(), {'B', 'I', 'G', 'F'});
    uint32_t total = at;
    for (const auto &e : entries)
        total += uint32_t(e.second.size());
    uint8_t totalLe[4] = {uint8_t(total), uint8_t(total >> 8), uint8_t(total >> 16),
                          uint8_t(total >> 24)};
    out.insert(out.end(), totalLe, totalLe + 4);
    uint32_t count = uint32_t(entries.size());
    uint8_t countBe[4] = {uint8_t(count >> 24), uint8_t(count >> 16), uint8_t(count >> 8),
                          uint8_t(count)};
    out.insert(out.end(), countBe, countBe + 4);
    uint32_t first = 16;
    uint8_t firstBe[4] = {uint8_t(first >> 24), uint8_t(first >> 16), uint8_t(first >> 8),
                          uint8_t(first)};
    out.insert(out.end(), firstBe, firstBe + 4);
    for (const auto &e : entries) {
        uint8_t offsetBe[4] = {uint8_t(at >> 24), uint8_t(at >> 16), uint8_t(at >> 8),
                               uint8_t(at)};
        uint32_t size = uint32_t(e.second.size());
        uint8_t sizeBe[4] = {uint8_t(size >> 24), uint8_t(size >> 16), uint8_t(size >> 8),
                             uint8_t(size)};
        out.insert(out.end(), offsetBe, offsetBe + 4);
        out.insert(out.end(), sizeBe, sizeBe + 4);
        out.insert(out.end(), e.first.begin(), e.first.end());
        out.push_back(0);
        at += size;
    }
    for (const auto &e : entries)
        out.insert(out.end(), e.second.begin(), e.second.end());
    return out;
}

// ============================================================================
// Driver
// ============================================================================

// AsciiString::endsWithNoCase
static bool endsWithNoCase(const std::string &s, const std::string &suffix) {
    if (s.size() < suffix.size())
        return false;
    size_t at = s.size() - suffix.size();
    for (size_t i = 0; i < suffix.size(); ++i) {
        if (::tolower(static_cast<unsigned char>(s[at + i])) !=
            ::tolower(static_cast<unsigned char>(suffix[i])))
            return false;
    }
    return true;
}

// BIGFileSystem::loadArchiveFilesFromDirectory (with the INIZH skip)
static bool loadArchiveFilesFromDirectory(FileSystem &fs,
                                          const std::map<std::string, std::vector<uint8_t>> &bigDefs,
                                          const std::string &dir,
                                          const std::string &fileMask,
                                          bool overwrite) {
    FilenameList filenameList;
    fs.m_local.getFileListInDirectory(dir, "", fileMask, filenameList, true);

    bool actuallyAdded = false;
    for (const auto &it : filenameList) {
        // TheSuperHackers @bugfix bobtista 18/11/2025 skip duplicate INIZH.big
        if (endsWithNoCase(it, "Data\\INI\\INIZH.big") ||
            endsWithNoCase(it, "Data/INI/INIZH.big")) {
            continue;
        }

        auto def = bigDefs.find(it);
        if (def == bigDefs.end())
            continue;
        ArchiveFile *archiveFile = openArchiveFileBytes(it, def->second);
        if (archiveFile) {
            fs.m_archive.loadIntoDirectoryTree(archiveFile, overwrite);
            fs.m_archive.m_archiveFileMap[it] = archiveFile;
            fs.m_archive.m_owned.push_back(archiveFile);
            actuallyAdded = true;
        }
    }
    return actuallyAdded;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: gen_vfs_golden <out.txt> <scratch-dir>\n");
        return 2;
    }

    namespace fs = std::filesystem;
    std::string scratch = argv[2];
    std::error_code ec;
    fs::remove_all(scratch, ec);
    fs::create_directories(scratch, ec);

    FILE *out = fopen(argv[1], "w");
    if (!out) {
        fprintf(stderr, "cannot open %s\n", argv[1]);
        return 1;
    }
    fprintf(out,
            "# rzh-vfs golden vectors (parity level P0).\n"
            "# Generated by tools/differential/gen_vfs_golden.cpp: a verbatim\n"
            "# transcription of the engine's archive/overlay/routing logic,\n"
            "# with compressed payloads produced by the REAL EAC RefPack codec\n"
            "# (refencode.cpp/refdecode.cpp, linked from the game tree).\n"
            "# Local files are created under a scratch directory by both this\n"
            "# generator and the Rust replay. '-' is the empty byte string.\n"
            "# Regenerate with tools/differential/regenerate.sh.\n\n");

    // ---- path utilities --------------------------------------------------
    fprintf(out, "# path keys and orderings\n");
    const char *keyPaths[] = {
        "Data\\INI\\Weapon.ini", "data/INI/weapon.INI", "Data\\\\INI\\\\X.ini",
        "a//b/c.txt", "", "plain", "C:\\Dir\\\\File.txt", "Data\\",
        "Data\\INI\\..\\Weapon.ini", "UPPER\\LOWER\\MIXED.TXT",
    };
    for (const char *p : keyPaths) {
        char key[MAX_PATH_260];
        if (makePathKey(key, sizeof(key), p)) {
            fprintf(out, "pathkey %s %s\n", arg(p), arg(key));
        } else {
            fprintf(out, "pathkey %s FAIL\n", arg(p));
        }
    }
    std::string longPath(300, 'x');
    fprintf(out, "pathkey %s FAIL\n", arg(longPath));

    const char *cmpPairs[][2] = {
        {"Data\\INI\\A.ini", "data/ini/a.INI"}, {"a\\b", "a\\c"},
        {"a\\b", "a"}, {"a\\b", "ab"}, {"", "x"}, {"x", ""},
        {"Data\\INI\\", "data/ini"}, {"Zebra.txt", "apple.txt"},
    };
    for (auto &pair : cmpPairs) {
        fprintf(out, "cmp %s %s %d\n", arg(pair[0]), arg(pair[1]),
                comparePath(pair[0], pair[1]));
    }

    fprintf(out, "\n# wildcard matcher\n");
    const char *wildPairs[][2] = {
        {"weapon.ini", "*.ini"}, {"weapon.ini", "*"}, {"weapon.ini", "weapon*"},
        {"weapon.ini", "w*n*i"}, {"weapon.ini", "**"}, {"weapon.ini", "*.txt"},
        {"weapon.ini", "*x"}, {"weapon.ini", "*z*"}, {"weapon", "weapon*"},
        {"weapon.i", "weapon*"}, {"weapon", "weapon?"}, {"weaponx", "weapon?"},
        {"armor.ini", "a*.ini"}, {"armor.ini", "??????.ini"}, {"armor.ini", "??????*.ini"},
        {"", ""}, {"", "*"}, {"a", ""}, {"Weapon.ini", "*.INI"},
    };
    for (auto &pair : wildPairs) {
        fprintf(out, "wild %s %s %d\n", arg(pair[0]), arg(pair[1]),
                SearchStringMatches(pair[0], pair[1]) ? 1 : 0);
    }

    // ---- payloads ---------------------------------------------------------
    // Deterministic payloads of different shapes; several are wrapped in
    // the EAR header with the REAL encoder (the form BIG entries carry).
    std::vector<uint8_t> payloadWeapon = ini_like(48);
    std::vector<uint8_t> payloadArmor = ini_like(31);
    std::vector<uint8_t> payloadModel = lcg_bytes(3000, 3);
    std::vector<uint8_t> payloadTexture = lcg_bytes(700, 5);
    std::vector<uint8_t> payloadAudio = lcg_bytes(1200, 2);
    std::vector<uint8_t> payloadTiny = {0x00, 0xff, 0x7f, 0x80, 0x01};
    std::vector<uint8_t> payloadOne = {0x5a};

    fprintf(out, "\n# compression dispatch (EAR payloads from the real encoder)\n");
    std::vector<std::vector<uint8_t>> compSamples = {
        compressRefpack(payloadWeapon), compressRefpack(payloadArmor),
        compressRefpack(payloadModel), compressRefpack(payloadTiny),
        compressRefpack(payloadOne), compressRefpack({}),
        std::vector<uint8_t>{'N', 'O', 'X', 0, 1, 0, 0, 0, 9, 9},
        std::vector<uint8_t>{'Z', 'L', '1', 0, 1, 0, 0, 0, 9, 9},
        std::vector<uint8_t>{'Z', 'L', '9', 0, 1, 0, 0, 0, 9, 9},
        std::vector<uint8_t>{'E', 'A', 'B', 0, 1, 0, 0, 0, 9, 9},
        std::vector<uint8_t>{'E', 'A', 'H', 0, 1, 0, 0, 0, 9, 9},
        std::vector<uint8_t>{'p', 'l', 'a', 'i', 'n', 'd', 'a', 't', 'a'},
        std::vector<uint8_t>{'s', 'h', 'o', 'r', 't'},
        {},
    };
    std::vector<std::vector<uint8_t>> expectedDecomp = {
        payloadWeapon, payloadArmor, payloadModel, payloadTiny, payloadOne, {}, {}, {},
        {},            {},           {},           std::vector<uint8_t>{'p', 'l', 'a', 'i', 'n', 'd', 'a', 't', 'a'},
        std::vector<uint8_t>{'s', 'h', 'o', 'r', 't'}, {},
    };
    for (size_t i = 0; i < compSamples.size(); ++i) {
        const auto &sample = compSamples[i];
        fprintf(out, "comptype %s %s\n", hex(sample).c_str(),
                compressionName(getCompressionType(sample.data(), int(sample.size()))));
        fprintf(out, "uncsize %s %d\n", hex(sample).c_str(),
                getUncompressedSize(sample.data(), int(sample.size())));
        // decomp: the EAR dispatch only (the driver wires RefPack alone;
        // Zero Hour data carries no other codec)
        if (getCompressionType(sample.data(), int(sample.size())) == COMPRESSION_REFPACK) {
            int ulen = getUncompressedSize(sample.data(), int(sample.size()));
            std::vector<uint8_t> dest(static_cast<size_t>(ulen));
            int ret = decompressData(sample.data(), int(sample.size()), dest.data(), ulen);
            if (ret == ulen) {
                fprintf(out, "decomp %s %s\n", hex(sample).c_str(), hex(dest).c_str());
            } else {
                fprintf(out, "decomp %s -\n", hex(sample).c_str());
            }
        }
        // and the consumer flow over every sample
        std::vector<uint8_t> served;
        if (cachedInputStreamRead(sample, served)) {
            fprintf(out, "decompofbytes %s %s\n", hex(sample).c_str(), hex(served).c_str());
        } else {
            fprintf(out, "decompofbytes %s -\n", hex(sample).c_str());
        }
        (void)expectedDecomp;
    }

    // ---- archives and local files ------------------------------------------
    fprintf(out, "\n# archives (built by this generator; hex is the image both\n"
                 "# sides mount) and the local files created under the scratch root\n");

    std::map<std::string, std::vector<uint8_t>> bigDefs;
    bigDefs["Base.big"] = buildBig({
        {"Data\\INI\\Weapon.ini", compressRefpack(payloadWeapon)},
        {"Data\\INI\\Armor.ini", compressRefpack(payloadArmor)},
        {"Data\\INI\\MixedCase.INI", compressRefpack(payloadTiny)},
        {"Data\\INI\\Weapon.ini", compressRefpack(payloadOne)}, // duplicate: last wins
        {"Data\\Audio\\Alert.wav", payloadAudio},                // plain (uncompressed)
        {"Data\\Empty.bin", {}},                                 // zero-size entry
        {"Art\\W3D\\Tank.w3d", compressRefpack(payloadModel)},
        {"Art\\Textures\\Sand.tga", compressRefpack(payloadTexture)},
        {"Art\\Textures\\grass.tga", payloadTexture}, // plain, case-paired names
        {"Audio/Music/Track01.wav", payloadAudio},    // forward slashes in the entry
        {"rootfile.txt", payloadTiny},
    });
    bigDefs["Patch.big"] = buildBig({
        {"Data\\INI\\Weapon.ini", compressRefpack(payloadTiny)},
        {"Art\\Textures\\Sand.tga", compressRefpack(payloadOne)},
        {"Data\\INI\\PatchOnly.ini", payloadTiny},
    });
    bigDefs["Mod.big"] = buildBig({
        {"Data\\INI\\Weapon.ini", compressRefpack(payloadArmor)},
        {"Data\\INI\\NewModule.ini", compressRefpack(payloadOne)},
    });
    // never mounted: the INIZH skip must pass it by
    bigDefs["Data\\INI\\INIZH.big"] = buildBig({
        {"Data\\INI\\Weapon.ini", compressRefpack(payloadModel)},
    });
    // a corrupt image: never mounted by mountdir (openArchiveFile fails)
    std::vector<uint8_t> broken = bigDefs["Base.big"];
    broken[0] = 'X';
    bigDefs["Broken.big"] = broken;

    for (const auto &def : bigDefs) {
        fprintf(out, "bigdef %s %s\n", def.first.c_str(), hex(def.second).c_str());
        // write it under the scratch root at the same game path
        std::string native = def.first;
        for (char &c : native)
            if (c == '\\')
                c = '/';
        std::string full = scratch + "/" + native;
        fs::create_directories(fs::path(full).parent_path(), ec);
        std::ofstream f(full, std::ios::binary);
        f.write(reinterpret_cast<const char *>(def.second.data()),
                std::streamsize(def.second.size()));
    }

    // local loose files (created under the scratch root)
    std::map<std::string, std::vector<uint8_t>> localDefs = {
        {"Data\\INI\\Armor.ini", std::vector<uint8_t>{'l', 'o', 'o', 's', 'e'}},
        {"Data\\INI\\LooseOnly.ini", std::vector<uint8_t>{'o', 'n', 'l', 'y'}},
        {"Notes\\readme.txt", payloadTiny},
    };
    for (const auto &def : localDefs) {
        fprintf(out, "localdef %s %s\n", def.first.c_str(), hex(def.second).c_str());
        std::string native = def.first;
        for (char &c : native)
            if (c == '\\')
                c = '/';
        std::string full = scratch + "/" + native;
        fs::create_directories(fs::path(full).parent_path(), ec);
        std::ofstream f(full, std::ios::binary);
        f.write(reinterpret_cast<const char *>(def.second.data()),
                std::streamsize(def.second.size()));
    }

    // ---- the mount/query script -------------------------------------------
    FileSystem vfs;
    vfs.m_local.m_root = scratch;

    auto emitOpen = [&](const std::string &path, int instance) {
        std::vector<uint8_t> data;
        if (vfs.openFile(path, uint8_t(instance), data)) {
            fprintf(out, "open %s %d %s\n", arg(path), instance, hex(data).c_str());
        } else {
            fprintf(out, "open %s %d -\n", arg(path), instance);
        }
    };
    auto emitDecompOf = [&](const std::string &path, int instance) {
        std::vector<uint8_t> data;
        if (vfs.openFile(path, uint8_t(instance), data)) {
            std::vector<uint8_t> served;
            if (cachedInputStreamRead(data, served)) {
                fprintf(out, "decompof %s %d %s\n", arg(path), instance,
                        hex(served).c_str());
            } else {
                fprintf(out, "decompof %s %d -\n", arg(path), instance);
            }
        } else {
            fprintf(out, "decompof %s %d -\n", arg(path), instance);
        }
    };
    auto emitExists = [&](const std::string &path, int instance) {
        fprintf(out, "exists %s %d %d\n", arg(path), instance,
                vfs.doesFileExist(path, uint8_t(instance)) ? 1 : 0);
    };
    auto emitInfo = [&](const std::string &path, int instance) {
        uint64_t size = 0;
        if (vfs.getFileInfo(path, uint8_t(instance), size)) {
            fprintf(out, "info %s %d %llu\n", arg(path), instance,
                    static_cast<unsigned long long>(size));
        } else {
            fprintf(out, "info %s %d -\n", arg(path), instance);
        }
    };
    auto emitList = [&](const std::string &dir, const std::string &pattern, bool recursive) {
        FilenameList list;
        vfs.getFileListInDirectory(dir, pattern, recursive, list);
        fprintf(out, "list %s %s %d %zu", arg(dir), arg(pattern), recursive ? 1 : 0,
                list.size());
        for (const auto &p : list)
            fprintf(out, " %s", arg(p));
        fprintf(out, "\n");
    };
    auto emitMountDir = [&](const std::string &dir, const std::string &mask, bool overwrite) {
        bool added = loadArchiveFilesFromDirectory(vfs, bigDefs, dir, mask, overwrite);
        fprintf(out, "mountdir %s %s %d %d\n", arg(dir), arg(mask), overwrite ? 1 : 0,
                added ? 1 : 0);
    };

    fprintf(out, "\n# first mount: every *.big under the root, path-key order,\n"
                    "# none overwriting (Broken fails to parse, INIZH skipped)\n");
    emitMountDir("", "*.big", false);

    fprintf(out, "\n# existence / open / info across instances (base + patch)\n");
    emitExists("Data\\INI\\Weapon.ini", 0);
    emitExists("Data\\INI\\Weapon.ini", 1);
    emitExists("Data\\INI\\Weapon.ini", 2);
    emitExists("data/ini/weapon.ini", 1);
    emitExists("Data\\INI\\Armor.ini", 0); // loose file present: shadowed
    emitExists("Data\\INI\\Armor.ini", 1);
    emitExists("Data\\INI\\Armor.ini", 2);
    emitExists("rootfile.txt", 0);
    emitExists("Data\\Empty.bin", 0);
    emitExists("Data\\INI\\Nope.ini", 0);
    emitExists("data\\ini\\mixedcase.ini", 0);
    emitExists("Audio\\Music\\Track01.wav", 0);

    fprintf(out, "\n# opens: raw stored bytes, then the decompress-on-read flow\n");
    emitOpen("Data\\INI\\Weapon.ini", 0); // the duplicate-name winner (payloadOne)
    emitOpen("data/ini/WEAPON.ini", 0);
    emitOpen("Data\\INI\\Weapon.ini", 1); // Patch.big's copy
    emitOpen("Data\\INI\\Weapon.ini", 2);
    emitOpen("Data\\INI\\Armor.ini", 0);  // the loose file
    emitOpen("Data\\INI\\Armor.ini", 1);  // Base.big's compressed copy
    emitOpen("Data\\INI\\Armor.ini", 2);
    emitOpen("Data\\INI\\MixedCase.INI", 0);
    emitOpen("Data\\Audio\\Alert.wav", 0);
    emitOpen("Data\\Empty.bin", 0);
    emitOpen("Art\\W3D\\Tank.w3d", 0);
    emitOpen("Art\\Textures\\Sand.tga", 0);
    emitOpen("Art\\Textures\\Sand.tga", 1);
    emitOpen("Art\\Textures\\grass.tga", 0);
    emitOpen("audio\\music\\track01.wav", 0);
    emitOpen("rootfile.txt", 0);
    emitOpen("Data\\INI\\Nope.ini", 0);
    emitOpen("Data\\INI\\PatchOnly.ini", 0);
    emitOpen("Notes\\readme.txt", 0);

    emitDecompOf("Data\\INI\\Weapon.ini", 0);
    emitDecompOf("Data\\INI\\Weapon.ini", 1);
    emitDecompOf("Data\\INI\\Armor.ini", 0); // loose (plain) file: served raw
    emitDecompOf("Data\\INI\\Armor.ini", 1); // compressed: served decompressed
    emitDecompOf("Data\\INI\\MixedCase.INI", 0);
    emitDecompOf("Data\\Audio\\Alert.wav", 0); // plain archive entry
    emitDecompOf("Data\\Empty.bin", 0);
    emitDecompOf("Art\\W3D\\Tank.w3d", 0);
    emitDecompOf("Art\\Textures\\Sand.tga", 1);
    emitDecompOf("audio\\music\\track01.wav", 0);
    emitDecompOf("Notes\\readme.txt", 0);
    emitDecompOf("Data\\INI\\Nope.ini", 0);

    fprintf(out, "\n# info sizes (timestamps are machine state, not pinned)\n");
    emitInfo("Data\\INI\\Weapon.ini", 0);
    emitInfo("Data\\INI\\Weapon.ini", 1);
    emitInfo("Data\\INI\\Armor.ini", 0);
    emitInfo("Data\\INI\\Armor.ini", 1);
    emitInfo("Data\\Empty.bin", 0);
    emitInfo("rootfile.txt", 0);
    emitInfo("Data\\Audio\\Alert.wav", 0);
    emitInfo("Data\\INI\\Nope.ini", 0);

    fprintf(out, "\n# listings (local + archive merged; the recursive flag\n"
                 "# applies to the local side only — archive listings always\n"
                 "# descend, like the C++)\n");
    emitList("", "*", true);
    emitList("Data\\INI\\", "*.ini", false);
    emitList("Data\\INI\\", "*.ini", true);
    emitList("Data\\", "*", false);
    emitList("Data\\", "*", true);
    emitList("Art\\", "*.tga", false);
    emitList("Art\\", "*.tga", true);
    emitList("Art\\Textures\\", "*", false);
    emitList("data\\", "*.wav", true);
    emitList("Nope\\", "*", true);
    emitList("Data\\INI\\", "a*.ini", true);
    emitList("Data\\INI\\", "????????.ini", true);
    emitList("Notes\\", "*", false);

    fprintf(out, "\n# the mod mounts on top (overwrite): instances flip\n");
    emitMountDir("", "*.big", true);

    emitExists("Data\\INI\\Weapon.ini", 0);
    emitExists("Data\\INI\\Weapon.ini", 1);
    emitExists("Data\\INI\\Weapon.ini", 2);
    emitExists("Data\\INI\\Weapon.ini", 3);
    emitOpen("Data\\INI\\Weapon.ini", 0);
    emitOpen("Data\\INI\\Weapon.ini", 1);
    emitOpen("Data\\INI\\Weapon.ini", 2);
    emitOpen("Data\\INI\\Weapon.ini", 3);
    emitDecompOf("Data\\INI\\Weapon.ini", 0);
    emitDecompOf("Data\\INI\\Weapon.ini", 1);
    emitOpen("Data\\INI\\NewModule.ini", 0);
    emitDecompOf("Data\\INI\\NewModule.ini", 0);
    emitList("Data\\INI\\", "*.ini", true);

    fprintf(out, "\n# a directory-scoped mount: only Patch.big under Mounts\\\n");
    // move Patch.big under a subdirectory and mount from there (non-recursive
    // mask against a subdir: exercises the dir + mask + recursion path)
    {
        std::string native = "Mounts/Patch.big";
        fs::create_directories(scratch + "/Mounts", ec);
        std::ofstream f(scratch + "/" + native, std::ios::binary);
        const auto &img = bigDefs["Patch.big"];
        f.write(reinterpret_cast<const char *>(img.data()), std::streamsize(img.size()));
        bigDefs["Mounts\\Patch.big"] = img;
        fprintf(out, "bigdef Mounts\\Patch.big %s\n", hex(img).c_str());
    }
    emitMountDir("Mounts\\", "*.big", true);
    emitOpen("Data\\INI\\Weapon.ini", 0);
    emitOpen("Data\\INI\\Weapon.ini", 1);
    emitOpen("Data\\INI\\Weapon.ini", 2);
    emitOpen("Data\\INI\\Weapon.ini", 3);
    emitOpen("Data\\INI\\Weapon.ini", 4);
    emitList("Data\\INI\\", "patch*.ini", true);

    fclose(out);

    fs::remove_all(scratch, ec);
    printf("gen_vfs_golden: wrote %s\n", argv[1]);
    return 0;
}
