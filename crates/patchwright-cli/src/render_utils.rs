pub(crate) fn push_plain_list(output: &mut String, heading: &str, items: &[String]) {
    output.push_str(&format!("{heading}:\n"));
    if items.is_empty() {
        output.push_str("  none\n\n");
        return;
    }
    for item in items {
        output.push_str(&format!("  {item}\n"));
    }
    output.push('\n');
}
