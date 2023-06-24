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

/* Based heavily on https://git.kernel.org/cgit/linux/kernel/git/stable/linux-stable.git/tree/drivers/power/test_power.c?id=refs/tags/v4.2.6 */
/* Fork from https://github.com/hoelzro/linux-fake-battery-module */

#include <asm/uaccess.h>
#include <linux/fs.h>
#include <linux/kernel.h>
#include <linux/miscdevice.h>
#include <linux/module.h>
#include <linux/power_supply.h>

static int fake_battery_get_property1(struct power_supply *psy,
                                      enum power_supply_property psp,
                                      union power_supply_propval *val);

static int fake_battery_get_property2(struct power_supply *psy,
                                      enum power_supply_property psp,
                                      union power_supply_propval *val);

static int fake_ac_get_property(struct power_supply *psy,
                                enum power_supply_property psp,
                                union power_supply_propval *val);

static struct battery_status {
    int status;
    int capacity_level;
    int capacity;
    int time_left;
} fake_battery_statuses[2] = {
    {
        .status = POWER_SUPPLY_STATUS_FULL,
        .capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL,
        .capacity = 100,
        .time_left = 3600,
    },
    {
        .status = POWER_SUPPLY_STATUS_FULL,
        .capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL,
        .capacity = 100,
        .time_left = 3600,
    },
};

static int ac_status = 1;

static char *fake_ac_supplies[] = {
    "BAT0",
    "BAT1",
};

static enum power_supply_property fake_battery_properties[] = {
    POWER_SUPPLY_PROP_STATUS,
    POWER_SUPPLY_PROP_CHARGE_TYPE,
    POWER_SUPPLY_PROP_HEALTH,
    POWER_SUPPLY_PROP_PRESENT,
    POWER_SUPPLY_PROP_TECHNOLOGY,
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

static enum power_supply_property fake_ac_properties[] = {
    POWER_SUPPLY_PROP_ONLINE,
};

static struct power_supply_desc descriptions[] = {
    {
        .name = "BAT0",
        .type = POWER_SUPPLY_TYPE_BATTERY,
        .properties = fake_battery_properties,
        .num_properties = ARRAY_SIZE(fake_battery_properties),
        .get_property = fake_battery_get_property1,
    },

    {
        .name = "BAT1",
        .type = POWER_SUPPLY_TYPE_BATTERY,
        .properties = fake_battery_properties,
        .num_properties = ARRAY_SIZE(fake_battery_properties),
        .get_property = fake_battery_get_property2,
    },

    {
        .name = "AC0",
        .type = POWER_SUPPLY_TYPE_MAINS,
        .properties = fake_ac_properties,
        .num_properties = ARRAY_SIZE(fake_ac_properties),
        .get_property = fake_ac_get_property,
    },
};

static struct power_supply_config configs[] = {
    {},
    {},
    {
        .supplied_to = fake_ac_supplies,
        .num_supplicants = ARRAY_SIZE(fake_ac_supplies),
    },
};

static struct power_supply *supplies[sizeof(descriptions) / sizeof(descriptions[0])];

static ssize_t control_device_read(struct file *file, char *buffer, size_t count, loff_t *ppos)
{
    static char *message = "fake battery information!";
    size_t message_len = strlen(message);

    if (count < message_len) {
        return -EINVAL;
    }

    if (*ppos != 0) {
        return 0;
    }

    if (copy_to_user(buffer, message, message_len)) {
        return -EINVAL;
    }

    *ppos = message_len;

    return message_len;
}

#define prefixed(s, prefix) (!strncmp((s), (prefix), sizeof(prefix) - 1))

static int handle_control_line(const char *line, int *ac_status, struct battery_status *batteries)
{
    char *value_p;
    long value;
    int status;

    value_p = strchrnul(line, '=');

    if (!value_p) {
        return -EINVAL;
    }

    value_p = skip_spaces(value_p + 1);

    status = kstrtol(value_p, 10, &value);

    if (status) {
        return status;
    }

    if (prefixed(line, "capacity")) {
        int battery_num = line[sizeof("capacity") - 1] - '0';
        if (battery_num != 0 && battery_num != 1) {
            return -ERANGE;
        }
        batteries[battery_num].capacity = value;
    } else if (prefixed(line, "charging")) {
        *ac_status = value;
    } else {
        return -EINVAL;
    }

    return 0;
}

static void handle_charge_changes(int ac_status, struct battery_status *battery)
{
    if (ac_status) {
        if (battery->capacity < 100) {
            battery->status = POWER_SUPPLY_STATUS_CHARGING;
        } else {
            battery->status = POWER_SUPPLY_STATUS_FULL;
        }
    } else {
        battery->status = POWER_SUPPLY_STATUS_DISCHARGING;
    }

    if (battery->capacity >= 98) {
        battery->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL;
    } else if (battery->capacity_level >= 70) {
        battery->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_HIGH;
    } else if (battery->capacity_level >= 30) {
        battery->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_NORMAL;
    } else if (battery->capacity_level >= 5) {
        battery->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_LOW;
    } else {
        battery->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_CRITICAL;
    }

    battery->time_left = 36 * battery->capacity;
}

static ssize_t control_device_write(struct file *file, const char *buffer, size_t count, loff_t *ppos)
{
    char kbuffer[1024]; /* limited by kernel frame size, 1K should be enough */
    char *buffer_cursor;
    char *newline;
    size_t bytes_left = count;

    int status;

    if (*ppos != 0) {
        printk(KERN_ERR "writes to /dev/fake_battery must be completed in a single system call\n");
        return -EINVAL;
    }

    if (count > 1024) {
        printk(KERN_ERR "Too much data provided to /dev/fake_battery (limit 1024 bytes)\n");
        return -EINVAL;
    }

    status = copy_from_user(kbuffer, buffer, count);

    if (status != 0) {
        printk(KERN_ERR "bad copy_from_user\n");
        return -EINVAL;
    }

    buffer_cursor = kbuffer;

    while ((newline = memchr(buffer_cursor, '\n', bytes_left))) {
        *newline = '\0';
        /* XXX this is non-atomic */
        status = handle_control_line(buffer_cursor, &ac_status, fake_battery_statuses);

        if (status) {
            return status;
        }

        bytes_left -= (newline - buffer_cursor) + 1;
        buffer_cursor = newline + 1;
    }

    handle_charge_changes(ac_status, &fake_battery_statuses[0]);
    handle_charge_changes(ac_status, &fake_battery_statuses[1]);

    power_supply_changed(supplies[0]);
    power_supply_changed(supplies[1]);
    power_supply_changed(supplies[2]);

    return count;
}

static struct file_operations control_device_ops = {
    .owner = THIS_MODULE,
    .read = control_device_read,
    .write = control_device_write,
};

static struct miscdevice control_device = {
    MISC_DYNAMIC_MINOR,
    "fake_battery",
    &control_device_ops,
};

static int fake_battery_generic_get_property(struct power_supply *psy,
                                             enum power_supply_property psp,
                                             union power_supply_propval *val,
                                             struct battery_status *status)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_MANUFACTURER:
            val->strval = "Linux";
            break;
        case POWER_SUPPLY_PROP_STATUS:
            val->intval = status->status;
            break;
        case POWER_SUPPLY_PROP_CHARGE_TYPE:
            val->intval = POWER_SUPPLY_CHARGE_TYPE_FAST;
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
        case POWER_SUPPLY_PROP_CAPACITY_LEVEL:
            val->intval = status->capacity_level;
            break;
        case POWER_SUPPLY_PROP_CAPACITY:
        case POWER_SUPPLY_PROP_CHARGE_NOW:
            val->intval = status->capacity;
            break;
        case POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN:
        case POWER_SUPPLY_PROP_CHARGE_FULL:
            val->intval = 100;
            break;
        case POWER_SUPPLY_PROP_TIME_TO_EMPTY_AVG:
        case POWER_SUPPLY_PROP_TIME_TO_FULL_NOW:
            val->intval = status->time_left;
            break;
        case POWER_SUPPLY_PROP_TEMP:
            val->intval = 26;
            break;
        case POWER_SUPPLY_PROP_VOLTAGE_NOW:
            val->intval = 3300;
            break;
        default:
            pr_info("%s: some properties deliberately report errors.\n", __func__);
            return -EINVAL;
    }
    return 0;
};

static int fake_battery_get_property1(struct power_supply *psy,
                                      enum power_supply_property psp,
                                      union power_supply_propval *val)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_MODEL_NAME:
            val->strval = "Fake battery 1";
            break;
        case POWER_SUPPLY_PROP_SERIAL_NUMBER:
            val->strval = "12345678";
            break;
        default:
            return fake_battery_generic_get_property(psy, psp, val, &fake_battery_statuses[0]);
    }
    return 0;
}

static int fake_battery_get_property2(struct power_supply *psy,
                                      enum power_supply_property psp,
                                      union power_supply_propval *val)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_MODEL_NAME:
            val->strval = "Fake battery 2";
            break;
        case POWER_SUPPLY_PROP_SERIAL_NUMBER:
            val->strval = "12345678";
            break;
        default:
            return fake_battery_generic_get_property(psy, psp, val, &fake_battery_statuses[1]);
    }
    return 0;
}

static int fake_ac_get_property(struct power_supply *psy,
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

static int __init fake_battery_init(void)
{
    int result;
    int i;

    result = misc_register(&control_device);
    if (result) {
        printk(KERN_ERR "Unable to register misc device!");
        return result;
    }

    for (i = 0; i < ARRAY_SIZE(descriptions); i++) {
        supplies[i] = power_supply_register(NULL, &descriptions[i], &configs[i]);
        if (IS_ERR(supplies[i])) {
            printk(KERN_ERR "Unable to register power supply %d in fake_battery\n", i);
            goto error;
        }
    }

    printk(KERN_INFO "loaded fake_battery module\n");
    return 0;

error:
    while (--i >= 0) {
        power_supply_unregister(supplies[i]);
    }
    misc_deregister(&control_device);
    return -1;
}

static void __exit fake_battery_exit(void)
{
    int i;

    misc_deregister(&control_device);

    for (i = ARRAY_SIZE(descriptions) - 1; i >= 0; i--) {
        power_supply_unregister(supplies[i]);
    }

    printk(KERN_INFO "unloaded fake_battery module\n");
}

module_init(fake_battery_init);
module_exit(fake_battery_exit);

MODULE_LICENSE("GPL");
