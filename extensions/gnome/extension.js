// SPDX-License-Identifier: GPL-3.0-or-later
//
// Extensão do GNOME Shell do Raiza: mostra no painel a bateria dos periféricos que o daemon
// `raizad` conhece. Só lê a API D-Bus (docs/DBUS.md); nunca escreve nada nos aparelhos.
//
// Atualiza só por sinais (DeviceAdded, DeviceRemoved, CapabilityChanged), sem polling. Se o
// daemon não está no barramento, o indicador fica oculto e volta quando o nome aparecer.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension, gettext as _} from 'resource:///org/gnome/shell/extensions/extension.js';

const DBUS_SERVICE = 'io.github.yurileo01.Raiza.Daemon';
const DBUS_PATH = '/io/github/yurileo01/Raiza';
const DBUS_INTERFACE = 'io.github.yurileo01.Raiza1';
const APP_ID = 'io.github.yurileo01.Raiza';

/** Ícone simbólico do tema para um nível de bateria (arredonda para baixo, como o Shell). */
function batteryIconName(percent, charging) {
    const level = Math.min(100, Math.max(0, Math.floor(percent / 10) * 10));
    if (charging)
        return level === 100 ? 'battery-level-100-charged-symbolic' : `battery-level-${level}-charging-symbolic`;
    return `battery-level-${level}-symbolic`;
}

/** Extrai a capacidade de bateria de um `DeviceInfo`, ou `null`. */
function batteryOf(capabilities) {
    if (!Array.isArray(capabilities))
        return null;
    const cap = capabilities.find(c => c && c.kind === 'battery');
    return cap ? parseBattery(cap) : null;
}

function parseBattery(cap) {
    if (typeof cap.percent !== 'number')
        return null;
    return {percent: cap.percent, charging: cap.charging === true};
}

const RaizaIndicator = GObject.registerClass(
class RaizaIndicator extends PanelMenu.Button {
    _init(app) {
        super._init(0.0, 'Raiza', false);

        this._icon = new St.Icon({
            icon_name: 'battery-level-100-symbolic',
            style_class: 'system-status-icon',
        });
        this.add_child(this._icon);

        this._devicesSection = new PopupMenu.PopupMenuSection();
        this.menu.addMenuItem(this._devicesSection);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        // Sem o app GTK instalado, o item some em vez de falhar.
        if (app)
            this.menu.addAction(_('Open Raiza'), () => app.activate());

        this.visible = false;
    }

    /** Redesenha a partir de `devices` (Map id → {name, battery}). */
    update(devices) {
        this._devicesSection.removeAll();

        const withBattery = [...devices.values()]
            .filter(d => d.battery !== null)
            .sort((a, b) => a.name.localeCompare(b.name));

        this.visible = withBattery.length > 0;
        if (!this.visible) {
            this.menu.close();
            return;
        }

        const lowest = withBattery.reduce((a, b) => (b.battery.percent < a.battery.percent ? b : a));
        this._icon.icon_name = batteryIconName(lowest.battery.percent, lowest.battery.charging);

        for (const device of withBattery) {
            const {percent, charging} = device.battery;
            const text = charging
                // TRANSLATORS: %s = device name, %d = battery percent
                ? _('%s: %d%%, charging').format(device.name, percent)
                // TRANSLATORS: %s = device name, %d = battery percent
                : _('%s: %d%%').format(device.name, percent);
            const item = new PopupMenu.PopupImageMenuItem(text, batteryIconName(percent, charging), {
                reactive: false,
                can_focus: false,
            });
            this._devicesSection.addMenuItem(item);
        }
    }
});

export default class RaizaExtension extends Extension {
    enable() {
        // id → {name, battery: {percent, charging} | null}
        this._devices = new Map();
        this._cancellable = new Gio.Cancellable();
        this._subscriptions = [];
        this._connection = null;

        this._indicator = new RaizaIndicator(Shell.AppSystem.get_default().lookup_app(`${APP_ID}.desktop`));
        Main.panel.addToStatusArea(this.uuid, this._indicator);

        this._watchId = Gio.bus_watch_name(
            Gio.BusType.SESSION,
            DBUS_SERVICE,
            Gio.BusNameWatcherFlags.NONE,
            connection => this._onNameAppeared(connection),
            () => this._onNameVanished());
    }

    disable() {
        if (this._watchId) {
            Gio.bus_unwatch_name(this._watchId);
            this._watchId = 0;
        }
        this._unsubscribe();
        this._cancellable?.cancel();
        this._cancellable = null;
        this._indicator?.destroy();
        this._indicator = null;
        this._devices = null;
    }

    _onNameAppeared(connection) {
        this._unsubscribe();
        this._connection = connection;
        const subscribe = (member, handler) => {
            const id = connection.signal_subscribe(
                DBUS_SERVICE, DBUS_INTERFACE, member, DBUS_PATH, null,
                Gio.DBusSignalFlags.NONE,
                (_conn, _sender, _path, _iface, _member, params) => {
                    try {
                        handler(...params.deepUnpack());
                    } catch (e) {
                        console.warn(`Raiza: sinal ${member} ilegível: ${e.message}`);
                    }
                });
            this._subscriptions.push(id);
        };
        // Assina antes de listar, para não perder um sinal emitido entre as duas coisas.
        subscribe('DeviceAdded', json => this._putDevice(JSON.parse(json)));
        subscribe('DeviceRemoved', id => {
            this._devices.delete(id);
            this._refresh();
        });
        subscribe('CapabilityChanged', (id, json) => {
            const cap = JSON.parse(json);
            const device = this._devices.get(id);
            if (!device || cap?.kind !== 'battery')
                return;
            device.battery = parseBattery(cap);
            this._refresh();
        });
        this._listDevices();
    }

    _onNameVanished() {
        this._unsubscribe();
        this._devices?.clear();
        this._refresh();
    }

    _unsubscribe() {
        for (const id of this._subscriptions)
            this._connection?.signal_unsubscribe(id);
        this._subscriptions = [];
        this._connection = null;
    }

    _listDevices() {
        const connection = this._connection;
        connection.call(
            DBUS_SERVICE, DBUS_PATH, DBUS_INTERFACE, 'ListDevices',
            null, new GLib.VariantType('(s)'), Gio.DBusCallFlags.NONE, -1,
            this._cancellable,
            (conn, res) => {
                let json;
                try {
                    [json] = conn.call_finish(res).deepUnpack();
                } catch (e) {
                    if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        console.warn(`Raiza: ListDevices falhou: ${e.message}`);
                    return;
                }
                if (!this._devices || this._connection !== connection)
                    return;
                try {
                    const list = JSON.parse(json);
                    this._devices.clear();
                    for (const info of Array.isArray(list) ? list : [])
                        this._storeDevice(info);
                } catch (e) {
                    console.warn(`Raiza: resposta de ListDevices ilegível: ${e.message}`);
                }
                this._refresh();
            });
    }

    _storeDevice(info) {
        if (!info || typeof info.id !== 'string')
            return;
        this._devices.set(info.id, {
            name: typeof info.name === 'string' && info.name ? info.name : info.id,
            battery: batteryOf(info.capabilities),
        });
    }

    _putDevice(info) {
        this._storeDevice(info);
        this._refresh();
    }

    _refresh() {
        if (this._indicator && this._devices)
            this._indicator.update(this._devices);
    }
}
