import random
import sys

def main(sizea, sizeb, pct, minsize, maxsize):
    print "&rand(" + str(sizea)+",13)[..],"
    print "&rand(" + str(sizeb)+",17)[..],"
    print "&["
    while sizea != 0 or sizeb != 0:
        cur_buf = random.randrange(minsize, maxsize + 1);
        is_a = random.randrange(0,100) < pct;
        index = 0 if is_a else 1
        if is_a:
            cur_buf = min(cur_buf, sizea)
            sizea -= cur_buf
        else:
            cur_buf = min(cur_buf, sizeb)
            sizeb -= cur_buf
        if cur_buf:
            print "                      (" + str(index) + "," + str(cur_buf) + "),"
    print "]"

if __name__ == "__main__":
    main(random.randrange(1, int(sys.argv[1])),random.randrange(1, int(sys.argv[2])),int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5]))
