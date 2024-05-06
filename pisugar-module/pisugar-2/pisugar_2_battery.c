/*
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

/* Based heavily on
 * https://git.kernel.org/cgit/linux/kernel/git/stable/linux-stable.git/tree/drivers/power/test_power.c?id=refs/tags/v4.2.6
 */
/*
 * Fork from https://github.com/hoelzro/linux-fake-battery-module
 * https://docs.kernel.org/power/power_supply_class.html
 */

#include <linux/fs.h>
#include <linux/i2c.h>
#include <linux/init.h>
#include <linux/kernel.h>
#include <linux/kthread.h>
#include <linux/miscdevice.h>
#include <linux/module.h>
#include <linux/power_supply.h>
#include <linux/sched.h>

#define BAT_I2C_BUS     0x01
#define IP5209_I2C_ADDR 0x75
#define IP5312_I2C_ADDR 0x75

#define TOTAL_LIFE_SECONDS        (3 * 60 * 60)
#define TOTAL_CHARGE              (2000 * 1000)  // uAH
#define TOTAL_CHARGE_FULL_SECONDS (60 * 60)

const int IP5209_CURVE[10][2] = {
    {4160, 100},
    {4050, 95},
    {4000, 80},
    {3920, 65},
    {3860, 40},
    {3790, 25},
    {3660, 10},
    {3520, 6},
    {3490, 3},
    {3100, 0},
};

const int IP5312_CURVE[10][2] = {
    {4100, 100},
    {4050, 95},
    {3900, 88},
    {3800, 77},
    {3700, 65},
    {3620, 55},
    {3580, 49},
    {3490, 25},
    {3320, 4},
    {3100, 0},
};

enum BAT_MODEL {
    STANDARD = 0,  // ip5209, for pi zero
    PRO = 1,       // ip5312, for pi 3/4
};

#define BAT_HIS_LEN 30
static int bat_voltage_his[BAT_HIS_LEN] = {0};  // mV

static short int i2c_bus = BAT_I2C_BUS;
static short int i2c_addr = IP5209_I2C_ADDR;
static short int bat_module = STANDARD;

module_param(i2c_bus, short, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP);
MODULE_PARM_DESC(i2c_bus, "I2C bus default 0x01");

module_param(i2c_addr, short, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP);
MODULE_PARM_DESC(i2c_addr, "I2C addr default 0x75");

module_param(bat_module, short, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP);
MODULE_PARM_DESC(bat_module, "PiSugar 2 model, 0 standard (pi zero), 1 pro (pi 3/4)");

static int pisugar_2_battery_get_property1(struct power_supply *psy,
                                           enum power_supply_property psp,
                                           union power_supply_propval *val);

static int pisugar_2_ac_get_property(struct power_supply *psy,
                                     enum power_supply_property psp,
                                     union power_supply_propval *val);

static struct task_struct *pisugar_2_monitor_task = NULL;

static struct battery_status {
    int status;
    int capacity_level;
    int capacity;   // %
    int time_left;  // seconds
    int voltage;    // uV
    int temperature;
} pisugar_2_battery_statuses[1] = {
    {
        .status = POWER_SUPPLY_STATUS_FULL,
        .capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL,
        .capacity = 100,
        .time_left = TOTAL_LIFE_SECONDS,
        .voltage = 4200 * 1000,  // uV
        .temperature = 30,
    },
};

static int ac_status = 1;

static char *pisugar_2_ac_supplies[] = {
    "BAT0",
};

static enum power_supply_property pisugar_2_battery_properties[] = {
    POWER_SUPPLY_PROP_STATUS,
    POWER_SUPPLY_PROP_CHARGE_TYPE,
    POWER_SUPPLY_PROP_HEALTH,
    POWER_SUPPLY_PROP_PRESENT,
    POWER_SUPPLY_PROP_TECHNOLOGY,
    POWER_SUPPLY_PROP_CHARGE_EMPTY,
    POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN,
    POWER_SUPPLY_PROP_CHARGE_FULL,
    POWER_SUPPLY_PROP_CHARGE_NOW,
    POWER_SUPPLY_PROP_CAPACITY,
    POWER_SUPPLY_PROP_CAPACITY_LEVEL,
    POWER_SUPPLY_PROP_TIME_TO_EMPTY_AVG,
    POWER_SUPPLY_PROP_TIME_TO_FULL_NOW,
    POWER_SUPPLY_PROP_MODEL_NAME,
    POWER_SUPPLY_PROP_MANUFACTURER,
    POWER_SUPPLY_PROP_SERIAL_NUMBER,
    POWER_SUPPLY_PROP_TEMP,
    POWER_SUPPLY_PROP_VOLTAGE_NOW,
};

static enum power_supply_property pisugar_2_ac_properties[] = {
    POWER_SUPPLY_PROP_ONLINE,
};

static struct power_supply_desc descriptions[] = {
    {
        .name = "BAT0",
        .type = POWER_SUPPLY_TYPE_BATTERY,
        .properties = pisugar_2_battery_properties,
        .num_properties = ARRAY_SIZE(pisugar_2_battery_properties),
        .get_property = pisugar_2_battery_get_property1,
    },

    {
        .name = "AC0",
        .type = POWER_SUPPLY_TYPE_MAINS,
        .properties = pisugar_2_ac_properties,
        .num_properties = ARRAY_SIZE(pisugar_2_ac_properties),
        .get_property = pisugar_2_ac_get_property,
    },
};

static struct power_supply_config configs[] = {
    {},
    {},
    {
        .supplied_to = pisugar_2_ac_supplies,
        .num_supplicants = ARRAY_SIZE(pisugar_2_ac_supplies),
    },
};

static struct power_supply *supplies[sizeof(descriptions) / sizeof(descriptions[0])];

#define prefixed(s, prefix) (!strncmp((s), (prefix), sizeof(prefix) - 1))

static int pisugar_2_battery_generic_get_property(struct power_supply *psy,
                                                  enum power_supply_property psp,
                                                  union power_supply_propval *val,
                                                  struct battery_status *status)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_MANUFACTURER:
            val->strval = "PiSugar";
            break;
        case POWER_SUPPLY_PROP_STATUS:
            val->intval = status->status;
            break;
        case POWER_SUPPLY_PROP_CHARGE_TYPE:
            val->intval = POWER_SUPPLY_CHARGE_TYPE_STANDARD;
            break;
        case POWER_SUPPLY_PROP_HEALTH:
            val->intval = POWER_SUPPLY_HEALTH_GOOD;
            break;
        case POWER_SUPPLY_PROP_PRESENT:
            val->intval = 1;
            break;
        case POWER_SUPPLY_PROP_TECHNOLOGY:
            val->intval = POWER_SUPPLY_TECHNOLOGY_LION;
            break;
        case POWER_SUPPLY_PROP_CAPACITY:
            val->intval = status->capacity;
            break;
        case POWER_SUPPLY_PROP_CAPACITY_LEVEL:
            val->intval = status->capacity_level;
            break;
        case POWER_SUPPLY_PROP_CHARGE_EMPTY:
            val->intval = 0;
            break;
        case POWER_SUPPLY_PROP_CHARGE_NOW:
            val->intval = status->capacity * TOTAL_CHARGE / 100;
            break;
        case POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN:
        case POWER_SUPPLY_PROP_CHARGE_FULL:
            val->intval = TOTAL_CHARGE;
            break;
        case POWER_SUPPLY_PROP_TIME_TO_EMPTY_AVG:
            val->intval = status->time_left;
            break;
        case POWER_SUPPLY_PROP_TIME_TO_FULL_NOW:
            val->intval = (100 - status->capacity) * TOTAL_CHARGE_FULL_SECONDS / 100;
            break;
        case POWER_SUPPLY_PROP_TEMP:
            val->intval = status->temperature;
            break;
        case POWER_SUPPLY_PROP_VOLTAGE_NOW:
            val->intval = status->voltage;
            break;
        default:
            pr_info("%s: some properties deliberately report errors.\n", __func__);
            return -EINVAL;
    }
    return 0;
};

static int pisugar_2_battery_get_property1(struct power_supply *psy,
                                           enum power_supply_property psp,
                                           union power_supply_propval *val)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_MODEL_NAME:
            val->strval = "PiSugar battery 0";
            break;
        case POWER_SUPPLY_PROP_SERIAL_NUMBER:
            val->strval = "";
            break;
        default:
            return pisugar_2_battery_generic_get_property(psy, psp, val, &pisugar_2_battery_statuses[0]);
    }
    return 0;
}

static int pisugar_2_ac_get_property(struct power_supply *psy,
                                     enum power_supply_property psp,
                                     union power_supply_propval *val)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_ONLINE:
            val->intval = ac_status;
            break;
        default:
            return -EINVAL;
    }
    return 0;
}

#define CHECK_VALID(val) ((val) >= 0 && (val) <= 255)

static void push_bat_voltage(int vol)
{
    for (int i = 0; i < BAT_HIS_LEN - 1; i++) {
        bat_voltage_his[i] = bat_voltage_his[i + 1];
    }
    bat_voltage_his[BAT_HIS_LEN - 1] = vol;
}

int get_bat_avg_voltage(void)
{
    long vol_sum = 0;
    for (int i = 0; i < BAT_HIS_LEN; i++) {
        vol_sum += bat_voltage_his[i];
    }
    return (int)(vol_sum / BAT_HIS_LEN);
}

static void update_bat_capacity_level_and_status(void)
{
    // capacity level
    int cap = pisugar_2_battery_statuses->capacity;
    if (cap > 95) {
        pisugar_2_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL;
    } else if (cap > 85) {
        pisugar_2_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_HIGH;
    } else if (cap > 40) {
        pisugar_2_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_NORMAL;
    } else if (cap > 30) {
        pisugar_2_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_LOW;
    } else {
        pisugar_2_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_CRITICAL;
    }

    // bat status
    if (ac_status) {
        if (cap > 95) {
            pisugar_2_battery_statuses->status = POWER_SUPPLY_STATUS_FULL;
        } else {
            pisugar_2_battery_statuses->status = POWER_SUPPLY_STATUS_CHARGING;
        }
    } else {
        pisugar_2_battery_statuses->status = POWER_SUPPLY_STATUS_DISCHARGING;
    }
}

static void ip5209_monitor_once(struct i2c_client *pisugar_2_client)
{
    int vol_low, vol_high, cap, charging_flags;
    int vol, vol_avg;  // mV

    // read voltage
    vol_low = i2c_smbus_read_byte_data(pisugar_2_client, 0xa2);
    vol_high = i2c_smbus_read_byte_data(pisugar_2_client, 0xa3);
    if (!CHECK_VALID(vol_high) || !CHECK_VALID(vol_low)) {
        return;
    }
    if ((vol_high & 0x20) == 0x20) {
        vol = 2600 - (long)((~vol_low) + (~(vol_high & 0x1F)) * 256 + 1) * 27 / 100;
    } else {
        vol = 2600 + (long)(vol_low + vol_high * 256) * 27 / 100;
    }
    push_bat_voltage(vol);
    vol_avg = get_bat_avg_voltage();                       // mV
    pisugar_2_battery_statuses->voltage = vol_avg * 1000;  // uV

    // capacity
    cap = 0;
    for (int i = 0; i < ARRAY_SIZE(IP5209_CURVE); i++) {
        if (vol_avg >= IP5209_CURVE[i][0]) {
            cap = IP5209_CURVE[i][1];
        }
        if (i > 0) {
            int vol_diff_v = vol_avg - IP5209_CURVE[i][0];
            int k = (IP5209_CURVE[i - 1][1] - IP5209_CURVE[i][1]) / (IP5209_CURVE[i - 1][0] - IP5209_CURVE[i][0]);
            cap += (int)(k * vol_diff_v);
        }
    }
    pisugar_2_battery_statuses->capacity = cap;

    // charging status
    charging_flags = i2c_smbus_read_byte_data(pisugar_2_client, 0x55);
    ac_status = (charging_flags & 0x10) > 0 ? 1 : 0;

    update_bat_capacity_level_and_status();
}

static void ip5312_monitor_once(struct i2c_client *pisugar_2_client)
{
    int vol_low, vol_high, cap, charging_flags;
    int vol, vol_avg;

    // read voltage
    vol_low = i2c_smbus_read_byte_data(pisugar_2_client, 0xd0);
    vol_high = i2c_smbus_read_byte_data(pisugar_2_client, 0xd1);
    if (!CHECK_VALID(vol_high) || !CHECK_VALID(vol_low)) {
        return;
    }
    vol = 2600 + (long)(vol_low + (vol_high & (0x1F)) * 256) * 27 / 100;

    push_bat_voltage(vol);
    vol_avg = get_bat_avg_voltage();                       // mV
    pisugar_2_battery_statuses->voltage = vol_avg * 1000;  // uV

    // capacity
    cap = 0;
    for (int i = 0; i < ARRAY_SIZE(IP5312_CURVE); i++) {
        if (vol_avg >= IP5312_CURVE[i][0]) {
            cap = IP5312_CURVE[i][1];
        }
        if (i > 0) {
            int vol_diff_v = vol_avg - IP5312_CURVE[i][0];
            int k = (IP5312_CURVE[i - 1][1] - IP5312_CURVE[i][1]) / (IP5312_CURVE[i - 1][0] - IP5312_CURVE[i][0]);
            cap += (int)(k * vol_diff_v);
        }
    }
    pisugar_2_battery_statuses->capacity = cap;

    // charging status
    charging_flags = i2c_smbus_read_byte_data(pisugar_2_client, 0x58);
    ac_status = (charging_flags & 0x10) > 0 ? 1 : 0;

    update_bat_capacity_level_and_status();
}

static int pisugar_2_monitor(void *args)
{
    struct i2c_client *pisugar_2_client = NULL;
    struct i2c_adapter *adapter = NULL;
    struct i2c_board_info board_info = {I2C_BOARD_INFO("pisugar_2_battery", i2c_addr)};

    // create i2c pisugar_2_client
    adapter = i2c_get_adapter(i2c_bus);
    if (adapter == NULL) {
        printk(KERN_ERR "Unable to get i2c adapter!");
        return -1;
    }
    pisugar_2_client = i2c_new_client_device(adapter, &board_info);
    if (pisugar_2_client == NULL) {
        printk(KERN_ERR "Unable to create i2c client!");
        return -1;
    }

    while (true) {
        set_current_state(TASK_UNINTERRUPTIBLE);
        if (kthread_should_stop()) break;

        if (bat_module == STANDARD) {
            ip5209_monitor_once(pisugar_2_client);
        }
        if (bat_module == PRO) {
            ip5312_monitor_once(pisugar_2_client);
        }

    sleep:
        set_current_state(TASK_RUNNING);
        schedule_timeout(HZ);
    }

    i2c_unregister_device(pisugar_2_client);
    pisugar_2_client = NULL;

    return 0;
}

static int __init pisugar_2_battery_init(void)
{
    int result;
    int i;

    for (int i = 0; i < BAT_HIS_LEN; i++) {
        bat_voltage_his[i] = 4200;
    }

    // create a monitor kthread
    pisugar_2_monitor_task = kthread_run(pisugar_2_monitor, NULL, "pisugar_2_monitor");
    if (pisugar_2_monitor_task == NULL) {
        goto error;
    }

    // register power supply
    for (i = 0; i < ARRAY_SIZE(descriptions); i++) {
        supplies[i] = power_supply_register(NULL, &descriptions[i], &configs[i]);
        if (IS_ERR(supplies[i])) {
            printk(KERN_ERR "Unable to register power supply %d in pisugar_2_battery\n", i);
            goto error;
        }
    }

    printk(KERN_INFO "loaded pisugar_2_battery module\n");
    return 0;

error:
    if (pisugar_2_monitor_task) {
        kthread_stop(pisugar_2_monitor_task);
        pisugar_2_monitor_task = NULL;
    }
    while (--i >= 0) {
        power_supply_unregister(supplies[i]);
    }
    return -1;
}

static void __exit pisugar_2_battery_exit(void)
{
    int i;

    if (pisugar_2_monitor_task) {
        kthread_stop(pisugar_2_monitor_task);
        pisugar_2_monitor_task = NULL;
    }

    for (i = ARRAY_SIZE(descriptions) - 1; i >= 0; i--) {
        power_supply_unregister(supplies[i]);
    }

    printk(KERN_INFO "unloaded pisugar_2_battery module\n");
}

module_init(pisugar_2_battery_init);
module_exit(pisugar_2_battery_exit);

MODULE_AUTHOR("The PiSugar Team <pisugar.zero@gmail.com>");
MODULE_DESCRIPTION("PiSugar 2 battery driver");
MODULE_LICENSE("GPL");
