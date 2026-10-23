### Discovery Alive Message (with Endpoints)

```hex
5f 61 73 64 70 5f 76 01 01 05 00 00 01 02 03 04 
05 06 07 08 74 6d 6c 6e 00 00 00 18 00 00 00 00 
00 07 a1 20 00 00 00 00 00 00 00 00 00 00 00 00 
00 00 00 00 73 65 73 73 00 00 00 08 08 07 06 05 
04 03 02 01 73 74 73 74 00 00 00 11 01 00 00 00 
00 00 0f 42 40 00 00 00 00 00 1e 84 80 6d 65 70 
34 00 00 00 06 c0 a8 01 05 51 48 61 65 70 34 00 
00 00 06 c0 a8 01 05 51 49 
```

### Discovery ByeBye Message

```hex
5f 61 73 64 70 5f 76 01 03 00 00 00 01 02 03 04 
05 06 07 08 
```

### Measurement Ping Message (HostTime=1s, PrevGHostTime=0.5s)

```hex
5f 6c 69 6e 6b 5f 76 01 01 5f 5f 68 74 00 00 00 
08 00 00 00 00 00 0f 42 40 5f 70 67 74 00 00 00 
08 00 00 00 00 00 07 a1 20 
```

### Measurement Pong Message (GHostTime=1s, + PingPayload)

```hex
5f 6c 69 6e 6b 5f 76 01 02 73 65 73 73 00 00 00 
08 08 07 06 05 04 03 02 01 5f 5f 67 74 00 00 00 
08 00 00 00 00 00 0f 42 40 5f 6c 69 6e 6b 5f 76 
01 02 73 65 73 73 00 00 00 08 08 07 06 05 04 03 
02 01 5f 5f 67 74 00 00 00 08 00 00 00 00 00 0f 
42 40 
```

### Timeline Math

Given Tempo = 120.0 (500000us/beat), beatOrigin = 0.0, timeOrigin = 0us
- `beatAtTime(2000000us)` = 4
- `phaseAtTime(2000000us, 4.0)` = 0
- `timeAtBeat(4.0)` = 2000000us
- `phase(-1.0, 4.0)` = 3
- `phase(-5.0, 4.0)` = 3
- `phase(5.0, 4.0)` = 1

After `requestBeatAtTime(1.0, 3000000us, 4.0)` in session mode (respects quantum):
- Chooses time = 4500000us to align phase
- Resulting `beatOrigin` = -8
- Resulting `timeOrigin` = 0us
- Check: `beatAtTime(timeToForce)` = 1
