#include <ios>
#include <iostream>
#include <string>
#include <unordered_map>

std::unordered_map<int, int> values;
int days[static_cast<int>(1e5)];

struct Mode {
  int mxx{static_cast<int>(-1e9)};
  std::unordered_map<int, int>::iterator max_pointer{nullptr};

  void add(int value) {
    if (++values[value] > mxx) {
      mxx = values[value];
      max_pointer = values.find(value);
    }

    else if (values[value] == mxx && max_pointer->first > value) {
      max_pointer = values.find(value);
    }
  }

  int get_max() const {
    return (max_pointer != nullptr && max_pointer != values.end())
               ? max_pointer->first
               : -1;
  }

  Mode(int *begin, int *end) {
    values.clear();

    for (int *it{begin}; it != end; ++it) {
      if (*it)
        add(*it);
    }
  }
};

int main() {
  int k, N, M, day, temp;
  int buffer{0};
  bool printed{false};

  std::ios_base::sync_with_stdio(false);
  std::cin.tie(nullptr);

  std::cin >> k >> N >> M;

  for (int i{0}; i < M; ++i) {
    std::cin >> day >> temp;
    days[day - 1] = temp;
  }

  std::string result_buffer;
  result_buffer.reserve(2005);

  for (int i{0}; i < N; ++i) {
    if (printed)
      result_buffer.push_back(' ');
    else
      printed = true;

    if ((buffer = Mode(days + std::max(0, i - k), days + std::min(N, i + k + 1))
                      .get_max()) != -1)
      result_buffer += std::to_string(buffer);
    else
      result_buffer.push_back('X');

    if (result_buffer.length() == 2000) {
      std::cout << result_buffer;
      result_buffer.clear();
    }
  }

  std::cout << result_buffer;
}
