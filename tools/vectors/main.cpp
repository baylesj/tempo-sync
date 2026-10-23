/* Copyright 2023, Test Tooling. All rights reserved.
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, either version 2 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

#include <iostream>
#include <iomanip>
#include <vector>
#include <ableton/discovery/v1/Messages.hpp>
#include <ableton/link/v1/Messages.hpp>
#include <ableton/link/NodeState.hpp>
#include <ableton/link/Timeline.hpp>
#include <ableton/link/Phase.hpp>
#include <ableton/link/PayloadEntries.hpp>

using namespace ableton;
using namespace std::chrono;

void dump_hex(const std::string& label, const std::vector<uint8_t>& data) {
    std::cout << "### " << label << "\n\n```hex\n";
    for (size_t i = 0; i < data.size(); ++i) {
        std::cout << std::hex << std::setw(2) << std::setfill('0') << (int)data[i] << " ";
        if (i % 16 == 15 || i == data.size() - 1) std::cout << "\n";
    }
    std::cout << "```\n\n";
}

int main() {
    using namespace ableton::link;

    NodeId node_id({1, 2, 3, 4, 5, 6, 7, 8});
    SessionId session_id({8, 7, 6, 5, 4, 3, 2, 1});

    Timeline timeline;
    timeline.tempo = Tempo(120.0);
    timeline.beatOrigin = Beats(0.0);
    timeline.timeOrigin = microseconds(0);

    StartStopState stst(true, Beats(1.0), microseconds(2000000));

    NodeState state;
    state.nodeId = node_id;
    state.sessionId = session_id;
    state.timeline = timeline;
    state.startStopState = stst;

    auto payload = toPayload(state);
    
    discovery::v1::MessageBuffer buf;
    auto end_it = discovery::v1::aliveMessage(node_id, 5, payload, buf.begin());
    dump_hex("Discovery Alive Message", std::vector<uint8_t>(buf.begin(), end_it));

    end_it = discovery::v1::byeByeMessage(node_id, buf.begin());
    dump_hex("Discovery ByeBye Message", std::vector<uint8_t>(buf.begin(), end_it));

    // Measurement ping
    link::v1::MessageBuffer buf2;
    auto ping_payload = discovery::makePayload(HostTime(microseconds(1000000)), PrevGHostTime(microseconds(500000)));
    auto end_it2 = link::v1::pingMessage(ping_payload, buf2.begin());
    dump_hex("Measurement Ping Message", std::vector<uint8_t>(buf2.begin(), end_it2));

    // Measurement pong
    auto pong_payload = discovery::makePayload(SessionMembership{session_id}, GHostTime(microseconds(1000000)));
    auto end_it3 = link::v1::pongMessage(pong_payload, buf2.begin());
    // Append ping payload for complete pong representation
    end_it3 = std::copy(buf2.begin(), end_it2, end_it3);
    dump_hex("Measurement Pong Message", std::vector<uint8_t>(buf2.begin(), end_it3));

    // Timeline math
    std::cout << "### Timeline Math\n\n";
    std::cout << "Given Tempo = 120.0 (500000us/beat), beatOrigin = 0.0, timeOrigin = 0us\n";
    
    std::cout << std::dec;
    std::cout << "- `beatAtTime(2000000us)` = " << timeline.toBeats(microseconds(2000000)).floating() << "\n";
    std::cout << "- `phaseAtTime(2000000us, 4.0)` = " << phase(timeline.toBeats(microseconds(2000000)), Beats(4.0)).floating() << "\n";
    std::cout << "- `timeAtBeat(4.0)` = " << timeline.fromBeats(Beats(4.0)).count() << "us\n";
    
    // Test quantum edge cases
    std::cout << "- `phase(-1.0, 4.0)` = " << phase(Beats(-1.0), Beats(4.0)).floating() << "\n";
    std::cout << "- `phase(-5.0, 4.0)` = " << phase(Beats(-5.0), Beats(4.0)).floating() << "\n";
    std::cout << "- `phase(5.0, 4.0)` = " << phase(Beats(5.0), Beats(4.0)).floating() << "\n";

    return 0;
}
