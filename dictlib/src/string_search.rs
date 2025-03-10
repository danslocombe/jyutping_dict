use core::hash;
use std::i32;

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

    let mut matrix: Vec<i32> = Vec::new();
    matrix.resize(n * m, 0);

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

    let mut matrix: Vec<i32> = Vec::new();
    matrix.resize(n * m, 0);

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

pub fn string_indexof_linear_ignorecase(needle: &str, haystack: &str) -> Option<usize> {
    let needle_bs = needle.as_bytes();
    let haystack_bs = haystack.as_bytes();

    if (haystack_bs.len() < needle_bs.len()) {
        return None;
    }

    let end = haystack_bs.len() - needle_bs.len();

    for i in 0..end {
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
            let c0 = unsafe { needle_bs.as_ptr().offset(j as isize).read() };
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
        assert_eq!(None, string_indexof_linear_ignorecase("hello", "there"));
        assert_eq!(Some(1), string_indexof_linear_ignorecase("hello", " hello  "));
        assert_eq!(Some(5), string_indexof_linear_ignorecase("helLO", "😭 hello  "));
        assert_eq!(Some(5), string_indexof_linear_ignorecase("hello", "😭 HeLLo  "));
        assert_eq!(Some(9), string_indexof_linear_ignorecase("😭", "oh thats 😭 hello  "));
    }
}