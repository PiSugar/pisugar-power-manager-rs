# Configuration

## /etc/default/pisugar-server.default

Default args of pisugar-server systemd service, see `pisugar-server -h`.

## /etc/pisugar-server/config.json

Json format configuration file of pisugar-server:

    i2c_bus: i2c bus number, optional, default 1 (i.e. /dev/i2c-1)
    auto_wake_time: RTC wakeup time, optional, iso8601 format, year/month/day is ignored, e.g. 2020-01-01T01:01:01+00:00
    auto_wake_repeat: RTC wakup weekday repeat, optional, bit 0 = Sunday, bit 6 = Saturday, e.g. 127 (i.e. 0b0111_1111)
    single_tap_enable: 
    