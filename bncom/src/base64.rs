
pub fn base64_decode(input: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer: u32 = 0;
    let mut buffer_length = 0;

    for &byte in input.as_bytes() {
        if byte == b'=' {
            break; // Padding字符，结束解码
        }

        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue, // 忽略非Base64字符
        } as u32;

        buffer = (buffer << 6) | value;
        buffer_length += 6;

        if buffer_length >= 8 {
            buffer_length -= 8;
            let decoded_byte = (buffer >> buffer_length) as u8;
            output.push(decoded_byte);
        }
    }

    output
}
