import re
with open("crates/tempo-sync-core/src/engine.rs", "r") as f:
    data = f.read()

data = data.replace(
'''                        let mut offset = 0;
                        let mut valid = false;
                        if let Some(b_t1) = prev_ghost_time {
                            // offset1 = B_t2 - (A_t3 + A_t2) / 2.0
                            // offset2 = (B_t2 + B_t1) / 2.0 - A_t2
                            let off1 = b_t2.0 - (a_t3.0 + a_t2.0) / 2;
                            let off2 = (b_t2.0 + b_t1.0) / 2 - a_t2.0;
                            offset = (off1 + off2) / 2; // Average the two points?
                            valid = true;
                        } else {
                            // Only have offset1
                            offset = b_t2.0 - (a_t3.0 + a_t2.0) / 2;
                            valid = true;
                        }''',
'''                        let offset = if let Some(b_t1) = prev_ghost_time {
                            let off1 = b_t2.0 - (a_t3.0 + a_t2.0) / 2;
                            let off2 = (b_t2.0 + b_t1.0) / 2 - a_t2.0;
                            (off1 + off2) / 2
                        } else {
                            b_t2.0 - (a_t3.0 + a_t2.0) / 2
                        };
                        let valid = true;'''
)
with open("crates/tempo-sync-core/src/engine.rs", "w") as f:
    f.write(data)
