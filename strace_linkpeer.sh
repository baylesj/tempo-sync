strace -f tools/linkpeer/linkpeer > strace_lp.log 2>&1 &
sleep 5
kill $!
