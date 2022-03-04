#/bin/bash
TMP=$(i2cget -y 1 0x57 0x06)
#echo $TMP
#开机看门狗开启并喂狗 Turn on the watchdog and feed the dog
RST=$((0x18 | TMP ))
#echo $RST
#设置最大重启次数 Set the maximum number of restarts
i2cset -y 1 0x57 0x0a 10 
#写入寄存器 Write register
i2cset -y 1 0x57 0x06 $RST 

#该脚本应当设置为开机启动。每次开机只需要运行一次 The script should be set to boot. You only need to run it once per boot
