#/bin/bash
TMP=$(i2cget -y 1 0x57 0x06)
#echo $TMP
#看门狗开启WatchdogOn
RST=$((0x80 | TMP ))
#echo $RST
#设置超时时长10*2s Set timeout duration 10 * 2S
i2cset -y 1 0x57 0x07 10
#写入寄存器 Write register
i2cset -y 1 0x57 0x06 $RST
#i2cdump -y 1 0x57


#需要不断的周期性喂狗，否则系统会重新启动 You need to feed the dog regularly, otherwise the system will restart
while [ 1 ]; do
    TMP=$(i2cget -y 1 0x57 0x06)
    #echo $TMP  >> /home/pi/wdlog.txt
    #确保看门狗开启 Make sure the watchdog is on
    RST=$((0x80 | TMP ))
    #喂看门狗 feed watchdog
    RST=$((0x20 | TMP ))
    #echo $RST  >> /home/pi/wdlog.txt
    i2cset -y 1 0x57 0x06 $RST
    #i2cget -y 1 0x57 0x06  >> /home/pi/wdlog.txt
    #i2cdump -y 1 0x57 >> /home/pi/wdlog.txt
    sleep 1
    #Once a second
done
