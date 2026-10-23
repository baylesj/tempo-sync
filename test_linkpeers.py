import subprocess
import time

with open("lp1.log", "w") as lp1_log, open("lp2.log", "w") as lp2_log:
    lp1 = subprocess.Popen(["./tools/linkpeer/linkpeer"], stdin=subprocess.PIPE, stdout=lp1_log, stderr=subprocess.STDOUT)
    lp2 = subprocess.Popen(["./tools/linkpeer/linkpeer"], stdin=subprocess.PIPE, stdout=lp2_log, stderr=subprocess.STDOUT)
    
    time.sleep(3)
    
    lp1.stdin.write(b"tempo 100\n")
    lp1.stdin.flush()
    time.sleep(2)
    
    lp1.kill()
    lp2.kill()

with open("lp1.log", "r") as f:
    print("--- LP1 ---")
    print(f.read()[-500:])

with open("lp2.log", "r") as f:
    print("--- LP2 ---")
    print(f.read()[-500:])
