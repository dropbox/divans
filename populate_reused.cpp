#include <stdlib.h>
#include <stdio.h>
#include <iostream>
#include <string>
#include <map>
#include <vector>

std::map<std::pair<int, int>, int> uses;


void process_command(ssize_t &file_offset,
                     const std::string &command,
                     bool do_print) {
    int offset = 0;
    int distance = 0;
    switch(command[0]) {
      case 'w':
        if (do_print) {
            std::cout << command << std::endl;
        }
        break;
      case 'd':
        sscanf(command.c_str(), "dict %d", &offset);
        file_offset += offset;
        if (do_print) {
            std::cout << command << std::endl;
        }
        break;
      case 'i':
        sscanf(command.c_str(), "insert %d", &offset);
        file_offset += offset;
        if (do_print) {
            std::cout << command << std::endl;
        }
        break;
      case 'b':
        if (do_print) {
            std::cout << command <<  std::endl;
        }
        break;
      case 'c':
        sscanf(command.c_str(), "copy %d from %d", &offset, &distance);
        std::pair<int, int> key(file_offset - distance, offset);
        if (do_print) {
            bool reused = false;
            if (uses[key] > 1)  {
                reused = true;
            }
            key.second += 1;
            if (uses[key] > 0) {
                reused = true;
            }
            key.first += 1;
            if (uses[key] > 0) {
                reused = true;
            }
            key.first -= 1;
            key.second += 1;
            if (uses[key] > 0) {
                reused = true;
            }
            key.first += 1;
            if (uses[key] > 0) {
                reused = true;
            }
            key.first += 1;
            if (uses[key] > 0) {
                reused = true;
            }
            std::cout << "copy " << offset << " from "<< distance <<" " << (reused?"reused":"unused")<<  std::endl;
        } else {
            uses[key] += 1;
        }
        file_offset += offset;
        break;
    }
}
int main() {
    std::vector<std::string> commands;
    {
        std::string command;
        ssize_t file_offset = 0;
        while (std::getline(std::cin, command)) {
            commands.push_back(command);
            process_command(file_offset, command, false);
        }
    }
    std::vector<std::string>::iterator command;
    ssize_t second_file_offset = 0;
    for (command = commands.begin(); command != commands.end(); ++command) {
        process_command(second_file_offset,
                        *command,
                        true);
    }
}
