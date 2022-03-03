#/bin/bash
TMP=$(i2cget -y 1 0x57 0x06)
#echo $TMP
RST=$((0x80 | TMP ))#看门狗开启 Watchdog on
#echo $RST

i2cset -y 1 0x57 0x07 10 #设置超时时长10*2s Set timeout duration 10 * 2S
i2cset -y 1 0x57 0x06 $RST #写入寄存器 Write register
#i2cdump -y 1 0x57


#需要不断的周期性喂狗，否则系统会重新启动 You need to feed the dog regularly, otherwise the system will restart
while [ 1 ]; do
TMP=$(i2cget -y 1 0x57 0x06)
#echo $TMP  >> /home/pi/wdlog.txt
RST=$((0x80 | TMP )) #确保看门狗开启 Make sure the watchdog is on
RST=$((0x20 | TMP )) #喂看门狗 feed watchdog
#echo $RST  >> /home/pi/wdlog.txt
i2cset -y 1 0x57 0x06 $RST
#i2cget -y 1 0x57 0x06  >> /home/pi/wdlog.txt
#i2cdump -y 1 0x57 >> /home/pi/wdlog.txt
sleep 1
#Once a second
done
