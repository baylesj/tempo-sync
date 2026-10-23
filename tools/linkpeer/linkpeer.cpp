// This file is part of tempo-sync.
//
// This file is licensed GPLv2+ because it links against Ableton Link;
// it is test tooling and is never shipped or linked into the Rust crate
// or any browser.

#include <ableton/Link.hpp>

#include <chrono>
#include <csignal>
#include <cstdlib>
#include <iomanip>
#include <iostream>
#include <poll.h>
#include <sstream>
#include <string>
#include <unistd.h>
#include <vector>

namespace
{

volatile std::sig_atomic_t g_signal_received = 0;

void signalHandler(int /* signal */)
{
  g_signal_received = 1;
}

std::string trim(const std::string& s)
{
  const auto start = s.find_first_not_of(" \t\r\n");
  if (start == std::string::npos)
  {
    return "";
  }
  const auto end = s.find_last_not_of(" \t\r\n");
  return s.substr(start, end - start + 1);
}

void printSessionState(ableton::Link& link, double quantum)
{
  const auto now = link.clock().micros();
  const auto sessionState = link.captureAppSessionState();
  const auto tempo = sessionState.tempo();
  const auto beat = sessionState.beatAtTime(now, quantum);
  const auto phase = sessionState.phaseAtTime(now, quantum);
  const auto numPeers = link.numPeers();
  const auto isPlaying = sessionState.isPlaying();
  const auto timeForIsPlaying = sessionState.timeForIsPlaying().count();

  std::cout << "{\"host_time_us\": " << now.count()
            << ", \"tempo\": " << tempo
            << ", \"beat\": " << beat
            << ", \"phase\": " << phase
            << ", \"quantum\": " << quantum
            << ", \"num_peers\": " << numPeers
            << ", \"is_playing\": " << (isPlaying ? "true" : "false")
            << ", \"time_for_is_playing_us\": " << timeForIsPlaying
            << "}\n" << std::flush;
}

bool processCommandLine(const std::string& line,
                        ableton::Link& link,
                        double quantum)
{
  const std::string trimmed = trim(line);
  if (trimmed.empty())
  {
    return true;
  }

  std::istringstream iss(trimmed);
  std::string cmd;
  if (!(iss >> cmd))
  {
    return true;
  }

  if (cmd == "quit")
  {
    std::cout << "{\"ack\": \"quit\"}\n" << std::flush;
    return false;
  }
  else if (cmd == "tempo")
  {
    double bpm = 0.0;
    if (iss >> bpm && bpm > 0.0)
    {
      auto state = link.captureAppSessionState();
      state.setTempo(bpm, link.clock().micros());
      link.commitAppSessionState(state);
      std::cout << "{\"ack\": \"tempo\"}\n" << std::flush;
    }
    else
    {
      std::cout << "{\"ack\": \"error\", \"message\": \"invalid tempo value\"}\n" << std::flush;
    }
  }
  else if (cmd == "play")
  {
    auto state = link.captureAppSessionState();
    state.setIsPlaying(true, link.clock().micros());
    link.commitAppSessionState(state);
    std::cout << "{\"ack\": \"play\"}\n" << std::flush;
  }
  else if (cmd == "stop")
  {
    auto state = link.captureAppSessionState();
    state.setIsPlaying(false, link.clock().micros());
    link.commitAppSessionState(state);
    std::cout << "{\"ack\": \"stop\"}\n" << std::flush;
  }
  else if (cmd == "beat")
  {
    double beat = 0.0;
    std::string atWord;
    std::int64_t hostTimeUs = 0;
    if (iss >> beat >> atWord >> hostTimeUs && atWord == "at")
    {
      auto state = link.captureAppSessionState();
      state.requestBeatAtTime(beat, std::chrono::microseconds(hostTimeUs), quantum);
      link.commitAppSessionState(state);
      std::cout << "{\"ack\": \"beat\"}\n" << std::flush;
    }
    else
    {
      std::cout << "{\"ack\": \"error\", \"message\": \"expected beat <beat> at <host_time_us>\"}\n" << std::flush;
    }
  }
  else if (cmd == "force")
  {
    double beat = 0.0;
    std::string atWord;
    std::int64_t hostTimeUs = 0;
    if (iss >> beat >> atWord >> hostTimeUs && atWord == "at")
    {
      auto state = link.captureAppSessionState();
      state.forceBeatAtTime(beat, std::chrono::microseconds(hostTimeUs), quantum);
      link.commitAppSessionState(state);
      std::cout << "{\"ack\": \"force\"}\n" << std::flush;
    }
    else
    {
      std::cout << "{\"ack\": \"error\", \"message\": \"expected force <beat> at <host_time_us>\"}\n" << std::flush;
    }
  }
  else
  {
    std::cout << "{\"ack\": \"unknown\", \"command\": \"" << cmd << "\"}\n" << std::flush;
  }

  return true;
}

} // namespace

int main(int argc, char** argv)
{
  std::signal(SIGINT, signalHandler);
  std::signal(SIGTERM, signalHandler);

  double tempo = 120.0;
  double quantum = 4.0;
  int intervalMs = 100;
  double durationS = 0.0;

  for (int i = 1; i < argc; ++i)
  {
    const std::string arg = argv[i];
    if (arg == "--tempo" && i + 1 < argc)
    {
      tempo = std::stod(argv[++i]);
    }
    else if (arg == "--quantum" && i + 1 < argc)
    {
      quantum = std::stod(argv[++i]);
    }
    else if (arg == "--interval-ms" && i + 1 < argc)
    {
      intervalMs = std::stoi(argv[++i]);
      if (intervalMs < 1)
      {
        intervalMs = 1;
      }
    }
    else if (arg == "--duration-s" && i + 1 < argc)
    {
      durationS = std::stod(argv[++i]);
    }
    else if (arg == "--help" || arg == "-h")
    {
      std::cout << "Usage: " << argv[0] << " [options]\n"
                << "Options:\n"
                << "  --tempo <bpm>        Initial tempo (default: 120.0)\n"
                << "  --quantum <beats>    Quantum in beats (default: 4.0)\n"
                << "  --interval-ms <n>    Status report interval in ms (default: 100)\n"
                << "  --duration-s <n>     Run for n seconds, then exit (default: until EOF/quit)\n"
                << "  --help, -h           Show this help message\n";
      return 0;
    }
    else
    {
      std::cerr << "Unknown option: " << arg << "\n";
      return 1;
    }
  }

  ableton::Link link(tempo);
  link.enable(true);
  link.enableStartStopSync(true);

  const auto startTime = std::chrono::steady_clock::now();
  auto nextReport = startTime;

  std::string stdinBuffer;
  bool running = true;

  struct pollfd pfd;
  pfd.fd = STDIN_FILENO;
  pfd.events = POLLIN;

  while (running && g_signal_received == 0)
  {
    const auto now = std::chrono::steady_clock::now();

    if (durationS > 0.0)
    {
      const auto elapsedS =
        std::chrono::duration<double>(now - startTime).count();
      if (elapsedS >= durationS)
      {
        break;
      }
    }

    if (now >= nextReport)
    {
      printSessionState(link, quantum);
      nextReport = now + std::chrono::milliseconds(intervalMs);
    }

    const auto timeToNext =
      std::chrono::duration_cast<std::chrono::milliseconds>(nextReport - std::chrono::steady_clock::now()).count();
    const int pollTimeoutMs = (timeToNext > 0) ? static_cast<int>(timeToNext) : 0;

    pfd.revents = 0;
    const int ret = poll(&pfd, 1, pollTimeoutMs);

    if (ret > 0 && (pfd.revents & (POLLIN | POLLHUP | POLLERR)))
    {
      char buffer[1024];
      const ssize_t bytesRead = read(STDIN_FILENO, buffer, sizeof(buffer));
      if (bytesRead > 0)
      {
        stdinBuffer.append(buffer, static_cast<size_t>(bytesRead));
        size_t newlinePos = std::string::npos;
        while ((newlinePos = stdinBuffer.find('\n')) != std::string::npos)
        {
          const std::string line = stdinBuffer.substr(0, newlinePos);
          stdinBuffer.erase(0, newlinePos + 1);
          if (!processCommandLine(line, link, quantum))
          {
            running = false;
            break;
          }
        }
      }
      else if (bytesRead == 0)
      {
        // Stdin hit EOF. Process any remaining un-terminated input line first.
        if (!stdinBuffer.empty())
        {
          processCommandLine(stdinBuffer, link, quantum);
          stdinBuffer.clear();
        }
        running = false;
        break;
      }
      else
      {
        if (errno != EINTR && errno != EAGAIN)
        {
          running = false;
          break;
        }
      }
    }
  }

  return 0;
}
