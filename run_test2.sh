tail -f /dev/null | tools/linkpeer/linkpeer > lp.out 2>&1 &
LP_PID=$!
python3 run_test.py > run_test.out 2>&1 &
PY_PID=$!
sleep 5
kill $PY_PID $LP_PID
cat run_test.out
