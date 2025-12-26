pub fn local_levenshtein_ascii(query: &str, target: &str) -> i32
{
    if (query.len() == 0) {
        return 0;
    }

    if (target.len() == 0) {
        return query.len() as i32;
    }

    _local_levenshtein_bs(query.as_bytes(), target.as_bytes())
}

// Taken from the snapchat paper
// https://arxiv.org/pdf/2211.02767
pub fn _local_levenshtein_bs(query: &[u8], target: &[u8]) -> i32
{
    let n = query.len() + 1;
    let m = target.len() + 1;

    let mut matrix: Vec<i32> = vec![0; n * m];

    for i in 0..n {
        matrix[i*m] = i as i32;
    }

    let mut min_dist = i32::MAX;
    for i in 1..n {
        for j in 1..m {
            let cost = if (query[i-1] == target[j-1]) {
                0
            }
            else {
                1
            };

            let top = matrix[(i-1)*m + j] + 1;
            let left = matrix[i*m + j-1] + 1;
            let diag = matrix[(i-1)*m + j-1] + cost;

            let cur = top.min(left).min(diag);
            matrix[i*m + j] = cur;

            if (i == (n-1)) {
                min_dist = min_dist.min(cur)
            }
        }
    }

    min_dist
}


pub fn prefix_levenshtein_ascii(query: &str, target: &str) -> i32
{
    if (query.len() == 0) {
        return 0;
    }

    if (target.len() == 0) {
        return query.len() as i32;
    }

    _prefix_levenshtein_bs(query.as_bytes(), target.as_bytes())
}

pub fn _prefix_levenshtein_bs(query: &[u8], target: &[u8]) -> i32
{
    let n = query.len() + 1;
    let m = target.len() + 1;

    let mut matrix: Vec<i32> = vec![0; n * m];

    for i in 0..m {
        matrix[i] = i as i32;
    }

    for i in 0..n {
        matrix[i*m] = i as i32;
    }

    let mut min_dist = i32::MAX;
    for i in 1..n {
        for j in 1..m {
            let cost = if (query[i-1] == target[j-1]) {
                0
            }
            else {
                2
            };

            let top = matrix[(i-1)*m + j] + 1;
            let left = matrix[i*m + j-1] + 1;
            let diag = matrix[(i-1)*m + j-1] + cost;

            let cur = top.min(left).min(diag);
            matrix[i*m + j] = cur;

            if (i == (n-1)) {
                min_dist = min_dist.min(cur)
            }
        }
    }

    min_dist
}

pub fn string_indexof_linear_ignorecase(needle: &str, haystack_bs: &[u8]) -> Option<usize> {
    let needle_bs = needle.as_bytes();
    debug_assert!(needle_bs.len() > 0);

    if (haystack_bs.len() < needle_bs.len()) {
        return None;
    }

    let end = haystack_bs.len() - needle_bs.len();

    // For first pass with simd
    // For now we are using u64, should be using
    // 128bit https://doc.rust-lang.org/beta/core/arch/wasm32/index.html
    let first_needle_byte = needle_bs[0];
    let is_first_needle_byte_ascii_letter = (first_needle_byte >= b'a' && first_needle_byte <= b'z') || (first_needle_byte >= b'A' && first_needle_byte <= b'Z');

    let first_pass_key : Option<u64> = if (is_first_needle_byte_ascii_letter)
    {
        let x = (first_needle_byte | 0x20) as u64;
        Some(
            x << 56 |
            x << 48 |
            x << 40 |
            x << 32 |
            x << 24 |
            x << 16 |
            x << 8 |
            x << 0
        )
    }
    else {
        None
    };

    let mut i = 0;
    while i <= end {
        if (i + 8) < end {
            if let Some(key) = first_pass_key
            {
                let read = unsafe { haystack_bs.as_ptr().offset(i as isize).cast::<u64>().read() };
                const LOWERCASE_MASK : u64 = 0x2020202020202020;
                let lowercased = read | LOWERCASE_MASK;
                //let intersected = lowercased & key;
                // XOR the values - matching bytes become 0x00
                let xor = lowercased ^ key;

                // Check if any byte is zero using the classic SWAR (SIMD Within A Register) trick
                // This checks if there's a zero byte without testing each one individually
                const LO_U64: u64 = 0x0101010101010101;
                const HI_U64: u64 = 0x8080808080808080;

                let match_mask = (xor.wrapping_sub(LO_U64)) & !xor & HI_U64;
                let has_match = match_mask != 0;

                if has_match {
                    // Find first matching position
                    let byte_offset = match_mask.trailing_zeros() / 8;
                    i += byte_offset as usize;
                }
                else {
                    i += 8;
                    continue;
                }
            }
        }

        // Main match loop.
        //
        // Not actually correct, but good enough
        // the eq_ignore_ascii_case can trigger weird stuff as it may
        // lowercase a non-ascii char.

        // eq_ignore_ascii_case implementation
        //self.len() == other.len() && iter::zip(self, other).all(|(a, b)| a.eq_ignore_ascii_case(b))
        //
        // We expand this out as it's on the hot path
        // and we want the site to run well in debug builds
        // but rust's performance here is really not that great
        // outside release builds.

        let mut is_match = true;

        for j in 0..needle_bs.len()
        {
            let c0 = unsafe { needle_bs.as_ptr().add(j).read() };
            let c1 = unsafe { haystack_bs.as_ptr().offset(i as isize + j as isize).read() };

            if (!c0.eq_ignore_ascii_case(&c1))
            {
                is_match = false;
                break;
            }
        }

        if (is_match)
        {
            return Some(i);
        }

        i += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein()
    {
        assert_eq!(0, local_levenshtein_ascii("hi", "hike"));
        assert_eq!(0, local_levenshtein_ascii("ik", "hike"));
        assert_eq!(0, local_levenshtein_ascii("ke", "hike"));

        assert_eq!(1, local_levenshtein_ascii("ho", "hike"));
        assert_eq!(1, local_levenshtein_ascii("mike", "m!ke"));
        assert_eq!(1, local_levenshtein_ascii("mike", " m!ke"));
        assert_eq!(1, local_levenshtein_ascii("mike", "hi mcke!"));
    }

    #[test]
    fn test_prefix_levenshtein()
    {
        assert_eq!(0, prefix_levenshtein_ascii("hi", "hike"));
        assert_eq!(0, prefix_levenshtein_ascii("hike", "hike"));
        assert_eq!(1, prefix_levenshtein_ascii("ike", "hike"));
        assert_eq!(1, prefix_levenshtein_ascii("ik", "hike"));
        assert_eq!(1, prefix_levenshtein_ascii("bai", "baai"));
    }

    #[test]
    fn test_string_indexof()
    {
        assert_eq!(None, string_indexof_linear_ignorecase("hello", "there".as_bytes()));
        assert_eq!(Some(1), string_indexof_linear_ignorecase("hello", " hello  ".as_bytes()));
        assert_eq!(Some(5), string_indexof_linear_ignorecase("helLO", "😭 hello  ".as_bytes()));
        assert_eq!(Some(5), string_indexof_linear_ignorecase("hello", "😭 HeLLo  ".as_bytes()));
        assert_eq!(Some(9), string_indexof_linear_ignorecase("😭", "oh thats 😭 hello  ".as_bytes()));

        assert_eq!(Some(5), string_indexof_linear_ignorecase("one two", "test one two".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("one two", "one two".as_bytes()));
    }

    #[test]
    fn test_string_indexof_case_variations()
    {
        // All uppercase needle, lowercase haystack
        assert_eq!(Some(0), string_indexof_linear_ignorecase("HELLO", "hello world".as_bytes()));

        // All lowercase needle, uppercase haystack
        assert_eq!(Some(0), string_indexof_linear_ignorecase("hello", "HELLO WORLD".as_bytes()));

        // Mixed case variations
        assert_eq!(Some(0), string_indexof_linear_ignorecase("HeLLo", "hEllO WORLD".as_bytes()));
        assert_eq!(Some(6), string_indexof_linear_ignorecase("WoRlD", "HELLO world".as_bytes()));

        // Single character matches
        assert_eq!(Some(0), string_indexof_linear_ignorecase("A", "apple".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("a", "Apple".as_bytes()));
        assert_eq!(Some(3), string_indexof_linear_ignorecase("Z", "quiz".as_bytes()));
    }

    #[test]
    fn test_string_indexof_edge_cases()
    {
        // Empty haystack with non-empty needle
        assert_eq!(None, string_indexof_linear_ignorecase("hello", "".as_bytes()));

        // Needle longer than haystack
        assert_eq!(None, string_indexof_linear_ignorecase("hello world", "hi".as_bytes()));

        // Exact match
        assert_eq!(Some(0), string_indexof_linear_ignorecase("test", "test".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("TeSt", "tEsT".as_bytes()));

        // Match at the very end
        assert_eq!(Some(7), string_indexof_linear_ignorecase("end", "at the end".as_bytes()));
        assert_eq!(Some(7), string_indexof_linear_ignorecase("END", "at the end".as_bytes()));
    }

    #[test]
    fn test_string_indexof_ascii_boundaries()
    {
        // Test letters at ASCII boundaries
        assert_eq!(Some(0), string_indexof_linear_ignorecase("A", "a".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("Z", "z".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("a", "A".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("z", "Z".as_bytes()));

        // Mixed with numbers and symbols (should not be affected by case)
        assert_eq!(Some(0), string_indexof_linear_ignorecase("a1b", "A1B".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("test@123", "TEST@123".as_bytes()));

        // Numbers and symbols don't have case
        assert_eq!(Some(0), string_indexof_linear_ignorecase("123", "123".as_bytes()));
        assert_eq!(None, string_indexof_linear_ignorecase("123", "456".as_bytes()));
    }

    #[test]
    fn test_string_indexof_repeated_patterns()
    {
        // First occurrence should be returned
        assert_eq!(Some(0), string_indexof_linear_ignorecase("ab", "ababab".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("AB", "ababab".as_bytes()));

        // Multiple occurrences, case insensitive
        assert_eq!(Some(0), string_indexof_linear_ignorecase("hi", "hi HI hi".as_bytes()));
        assert_eq!(Some(0), string_indexof_linear_ignorecase("HI", "hi HI hi".as_bytes()));

        // Pattern with repeating characters
        assert_eq!(Some(2), string_indexof_linear_ignorecase("aaa", "bbaaabbb".as_bytes()));
        assert_eq!(Some(2), string_indexof_linear_ignorecase("AAA", "bbaaabbb".as_bytes()));
    }

    #[test]
    fn test_string_indexof_longer_strings()
    {
        // Longer realistic test cases
        let haystack = "The Quick Brown Fox Jumps Over The Lazy Dog";
        assert_eq!(Some(4), string_indexof_linear_ignorecase("quick", haystack.as_bytes()));
        assert_eq!(Some(4), string_indexof_linear_ignorecase("QUICK", haystack.as_bytes()));
        assert_eq!(Some(16), string_indexof_linear_ignorecase("fox", haystack.as_bytes()));
        assert_eq!(Some(35), string_indexof_linear_ignorecase("lazy dog", haystack.as_bytes()));

        // Not found in longer string
        assert_eq!(None, string_indexof_linear_ignorecase("cat", haystack.as_bytes()));

        // Partial match should not succeed
        assert_eq!(None, string_indexof_linear_ignorecase("foxes", haystack.as_bytes()));
    }
}
