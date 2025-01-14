#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>

int main() {
    int sound_;

    // 打开控制终端设备
    sound_fd = open("/dev/sound", O_RDWR);

    // 向控制终端写入数据
    
    return EXIT_SUCCESS;
}