import sys
data = sys.stdin.read();
magic = ['-551.62445', '413.18079']
rmagic = []#'846.69629', '-95.776024']
out_magic = [m for m in magic]
out_rmagic = [r for r in rmagic]

inc = 14
for i in range(0,30):
    for index in range(len(magic)):
        out_magic[index] = str(float(magic[index]) + inc * i)
    for index in range(len(rmagic)):
        out_rmagic[index] = str(float(rmagic[index]) - inc * i)
    fn = ''
    fn += str(int(i/1000))
    fn += str(int(i/100)%10)
    fn += str(int(i/10)%10)
    fn += str(int(i%10))
    temp = data
    for index in range(len(magic)):
        temp = temp.replace(magic[index], out_magic[index])
    for index in range(len(rmagic)):
        temp = temp.replace(rmagic[index], out_rmagic[index])
    print 'replacing', magic,'with',out_magic
    print 'replacing', rmagic,'with',out_rmagic
    with open(fn + '.svg', 'w') as out:
        out.write(temp);

