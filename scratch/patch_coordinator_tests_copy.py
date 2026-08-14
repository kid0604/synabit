import re

with open("scratch/coordinator.rs", "r") as f:
    content = f.read()

test_idx = content.find("mod tests {")
if test_idx != -1:
    before = content[:test_idx]
    after = content[test_idx:]
    after = after.replace("remote_position: 1,", 'remote_position: "1".to_string(),')
    content = before + after

with open("scratch/coordinator.rs", "w") as f:
    f.write(content)
