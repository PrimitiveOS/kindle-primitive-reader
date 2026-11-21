// The Rust caller supplies valid UTF-8, lengths, and a character-boundary offset.
// Prevent exceptions from crossing the C ABI, including allocation errors.
#define SRELL_NO_UNICODE_DATA
#include "srell.hpp"

namespace {
using Cancelled = bool (*)(const void*);
struct Stopped {};
struct WorkLimit {};
struct Budget {
    size_t* remaining;
    Cancelled cancelled;
    const void* context;
    void step() const {
        if (!*remaining) throw WorkLimit{};
        --*remaining;
        if ((*remaining & 1023) == 0 && cancelled(context)) throw Stopped{};
    }
};

// A bidirectional byte iterator lets the caller interrupt scanning inside
// SRELL, including inner loops of greedy expressions over long paragraphs.
struct Cursor {
    using iterator_category = std::bidirectional_iterator_tag;
    using value_type = char;
    using difference_type = std::ptrdiff_t;
    using pointer = const char*;
    using reference = const char&;
    const char* pos = nullptr;
    const Budget* budget = nullptr;
    reference operator*() const { return *pos; }
    Cursor& operator++() { budget->step(); ++pos; return *this; }
    Cursor& operator--() { budget->step(); --pos; return *this; }
    Cursor operator++(int) { auto old = *this; ++*this; return old; }
    Cursor operator--(int) { auto old = *this; --*this; return old; }
    friend bool operator==(Cursor a, Cursor b) { return a.pos == b.pos; }
    friend bool operator!=(Cursor a, Cursor b) { return a.pos != b.pos; }
};
}

extern "C" {
void* reader_regex_compile(const char* pattern, size_t length, int* error) noexcept {
    try {
        auto* expression = new srell::u8regex(pattern, pattern + length,
                                              srell::regex_constants::ECMAScript);
        expression->limit_counter = 1u << 15;
        return expression;
    } catch (const srell::regex_error& exc) {
        *error = exc.code();
    } catch (...) {
        *error = 109;
    }
    return nullptr;
}

void reader_regex_free(void* compiled) noexcept {
    delete static_cast<srell::u8regex*>(compiled);
}

// Search the original block, retaining the true start for ^, word boundaries
// and lookbehind. Suppress empty matches because they cannot select a passage.
int reader_regex_next(const void* compiled, const char* text, size_t length,
                      size_t offset, size_t* start, size_t* end, size_t* remaining,
                      Cancelled cancelled, const void* context) noexcept {
    try {
        const auto& expression = *static_cast<const srell::u8regex*>(compiled);
        srell::match_results<Cursor> found;
        const Budget budget{remaining, cancelled, context};
        const Cursor begin{text, &budget}, from{text + offset, &budget}, last{text + length, &budget};
        auto flags = srell::regex_constants::match_not_null;
        if (offset != 0)
            flags |= srell::regex_constants::match_prev_avail;
        if (!srell::regex_search(from, last, begin, found, expression, flags))
            return 0;
        *start = static_cast<size_t>(found[0].first.pos - text);
        *end = static_cast<size_t>(found[0].second.pos - text);
        return 1;
    } catch (const Stopped&) {
        return 0;
    } catch (const WorkLimit&) {
        return -111;
    } catch (const srell::regex_error& exc) {
        return -static_cast<int>(exc.code());
    } catch (...) {
        return -109;
    }
}
}
