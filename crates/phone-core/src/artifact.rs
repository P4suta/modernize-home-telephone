pub const MAX_RELATIVE_PATH_BYTES: usize = 256;

macro_rules! valid_position {
    ($bytes:expr, $index:expr) => {{
        let bytes = $bytes;
        let index = $index;
        let byte = bytes[index];
        matches!(byte,b'a'..=b'z'|b'A'..=b'Z'|b'0'..=b'9'|b'.'|b'_'|b'-'|b'/')
            && !(byte == b'/'
                && (index == 0 || index + 1 == bytes.len() || bytes[index - 1] == b'/'))
            && !(byte == b'.'
                && (index == 0 || bytes[index - 1] == b'/')
                && (index + 1 == bytes.len()
                    || bytes[index + 1] == b'/'
                    || (bytes[index + 1] == b'.'
                        && (index + 2 == bytes.len() || bytes[index + 2] == b'/'))))
    }};
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelativePath<'a>(&'a [u8]);
impl<'a> RelativePath<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        if accepts(bytes, 0) {
            Some(Self(bytes))
        } else {
            None
        }
    }
    pub const fn bytes(self) -> &'a [u8] {
        self.0
    }
}
pub(crate) fn accepts(bytes: &[u8], _proof_position: usize) -> bool {
    if !crate::bytes::nonempty_within(bytes.len(), MAX_RELATIVE_PATH_BYTES) {
        return false;
    }
    let mut remaining = bytes.len();
    let mut valid = true;
    #[cfg_attr(kani, kani::loop_invariant(remaining <= bytes.len() && (!valid || _proof_position >= bytes.len() - remaining || valid_position!(bytes, _proof_position))))]
    #[cfg_attr(kani, kani::loop_decreases(remaining))]
    while remaining > 0 {
        let index = bytes.len() - remaining;
        valid &= valid_position!(bytes, index);
        remaining = remaining.saturating_sub(1);
    }
    valid
}
