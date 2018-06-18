#include <iostream>
#include <fstream>
#include <string>
#include <vector>
#include <stdint.h>
#include <stdlib.h>
static bool starts_with(const std::string& s, const std::string& prefix) {
    return s.size() >= prefix.size() && s.compare(0, prefix.size(), prefix) == 0;
}
bool is_command(const std::string &line) {
    return starts_with(line, "copy") || starts_with(line, "insert") || starts_with(line, "dict");
}
bool on_whitelist0(const std::string &line) {
    return is_command(line);
}
bool on_whitelist1(const std::string &line) { 
    return starts_with(line, "pred") || starts_with(line, "ltype") || starts_with(line, "ctype") || starts_with(line, "dtype");
}

const std::string space = std::string(" ");
uint64_t byte_size(const std::string &line) {
    if (!is_command(line)) {
        return 0;
    }
    std::string::size_type where = line.find_first_of(space);
    
    std::string::size_type where1 = line.find_first_of(space, where + 1);
    return strtol(&line.data()[where + 1], NULL, 10);
}    
int main(int argc, char ** argv) {
    std::string line;
    std::vector<std::string> f0vec;
    std::vector<std::string> f1vec;
    std::ifstream afile(argv[1]);
    std::ifstream bfile(argv[2]);

    if(!afile) {
        std::cout<<"Error opening input0 file\n";
        return 1;
    }
    if(!bfile) {
        std::cout<<"Error opening input1 file\n";
        return 1;
    }
    while (std::getline(afile, line))
    {
        f0vec.push_back(line + "\n");
    }
    while (std::getline(bfile, line))
    {
        f1vec.push_back(line + "\n");
    }
    uint64_t f0count = 0;
    uint64_t f1count = 0;
    std::cout << "window 26 0 0 0\n";
    std::vector<std::string>::const_iterator f0 = f0vec.begin();
    std::vector<std::string>::const_iterator f1 = f1vec.begin();
    while (f0 != f0vec.end() || f1 != f1vec.end()) {
        if (((f0count >= f1count) && f1 != f1vec.end()) || (f0 == f0vec.end() && f1 != f1vec.end())) {
            const std::string &line = *f1;
            ++f1;
            if (on_whitelist1(line)) {
                std::cout << line;
            }
            f1count += byte_size(line);
        }
        if (((f0count < f1count) && f0 != f0vec.end()) || (f1 == f1vec.end() && f0 != f0vec.end())) {
            const std::string &line = *f0;
            ++f0;
            if (on_whitelist0(line)) {
                std::cout << line;
            }
            f0count += byte_size(line);
        }
    }
    return 0;
}
