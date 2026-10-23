python3 send_spoof.py &
PY_PID=$!
strace -f -e trace=recvfrom,recvmsg tools/linkpeer/linkpeer > lp.out 2> strace_lp.log &
LP_PID=$!
sleep 5
kill $LP_PID $PY_PID
wait $LP_PID $PY_PID 2>/dev/null
