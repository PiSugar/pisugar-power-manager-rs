# Configuration

## /etc/default/pisugar-server.default

Default args of pisugar-server systemd service, see `pisugar-server -h`.

## /etc/pisugar-server/config.json

Json format configuration file of pisugar-server:

    digest_auth     Enable http security (digest auth), e.g. ["admin", "<password>"]
                    default null (disable http security)

    i2c_bus         i2c bus number, optional, default 1 (i.e. /dev/i2c-1)

    auto_wake_time  RTC wakeup time, optional, iso8601 format
                    default null
                    year/month/day is ignored, e.g. 2020-01-01T01:01:01+00:00
    auto_wake_repeat RTC wakup weekday repeat, optional
                    default 0
                    bit 0 = Sunday, bit 6 = Saturday, e.g. 127 (0b0111_1111)

    single_tap_enable Enable single tap event(<0.5s), optional, default false
    single_tap_shell Shell script, (sh -c "<script>"), default ""
    double_tap_enable Enable double tap event, optional, default ""
    double_tap_shell See single_tap_shell
    long_tap_enable Enable long tap enent(>1s), optional, default false
    long_tap_shell  See single_tap_shell
    
    auto_shutdown_level Shutdown when battery is low, optional
                    default 0 (disable), suggested value 10
    auto_shutdown_delay Delay before auto shutdown (seconds), optional
                    default 0 (disable auto shutdown), suggested value 30
    auto_charging_range Enable charging between battery levels, optional
                    default null suggested value (60, 90)
                    Enable charging when battery < begin, then stop charging when battery > end
    full_charge_duration Keep charging (seconds) after battery is full, optional
                    default null, suggested value 120
    auto_power_on   Power on when power supply is restored, optional
                    default null
    soft_poweroff   PiSugar 3 only, pisugar notify pi to poweroff
                    default null
    soft_poweroff_shell Shell script of soft poweroff, default null

    auto_rtc_sync   Automatically sync rtc time (Every 10s)