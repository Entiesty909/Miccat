//! 枚举 WASAPI 端点。输出列表是全部播放设备，不靠你改过的友好名称过滤。

use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName, PKEY_DeviceInterface_FriendlyName,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::Win32::System::Variant::{VT_BSTR, VT_LPWSTR};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

use crate::error::Error;

use super::com;

/// 一只物理播放设备：友好名称 + 设备 ID。
#[derive(Clone, Debug)]
pub struct RenderDevice {
    /// WASAPI 设备 ID（不是 MME 下标）。
    pub id: String,
    /// 友好名称（用户改名后就是新名字）。
    pub name: String,
}

/// 物理麦克风：排除虚拟线缆录音端，避免把 CABLE Output 再混进去。
pub fn list_physical_mics() -> Result<Vec<RenderDevice>, Error> {
    Ok(list_endpoints(eCapture, eCommunications)?
        .into_iter()
        .filter(|d| !is_virtual_driver(d))
        .map(Endpoint::into_render)
        .collect())
}

/// UI 输出列表里的一项。
#[derive(Clone, Debug)]
pub struct DestDevice {
    /// ComboBox 文案（用当前友好名称）。
    pub label: String,
    /// 实际 WASAPI 渲染目标。
    pub render_id: String,
    /// 若能配到录音端，设默认麦用。
    pub capture_id: Option<String>,
    /// 游戏里应选择的麦克风名（也可能是你改过的名字）。
    pub capture_name: Option<String>,
    /// 驱动分组键：同一虚拟驱动的两个渲染端点不能同时打开。
    pub driver_family: String,
    /// 该端点是否来自虚拟音频驱动。
    pub virtual_driver: bool,
}

/// 输出列表：虚拟扬声器与虚拟麦克风各一条，不做「扬声器 → 麦」拼接。
/// `include_all` 为真时也列出物理播放设备；虚拟录音端只在找到对应播放端后才加入。
pub fn list_destinations(include_all: bool) -> Result<Vec<DestDevice>, Error> {
    let renders = list_endpoints(eRender, eConsole)?;
    let captures = list_endpoints(eCapture, eCommunications)?;
    let mut out = Vec::new();

    // 先列播放端，再将可配对的虚拟录音端作为独立选项追加；避免把配对关系伪装成一条 UI 映射。
    for r in &renders {
        if !include_all && !is_virtual_driver(r) {
            continue;
        }
        out.push(DestDevice {
            label: r.name.clone(),
            render_id: r.id.clone(),
            capture_id: None,
            capture_name: None,
            driver_family: driver_key_of(r),
            virtual_driver: is_virtual_driver(r),
        });
    }

    // 虚拟麦克风实际不能作为 WASAPI 渲染目标，因此保存其录音端 ID，同时指向驱动对应的播放端。
    for c in &captures {
        if !is_virtual_driver(c) {
            continue;
        }
        let Some(r) = paired_render(c, &renders) else {
            continue;
        };
        out.push(DestDevice {
            label: c.name.clone(),
            render_id: r.id.clone(),
            capture_id: Some(c.id.clone()),
            capture_name: Some(c.name.clone()),
            driver_family: driver_key_of(r),
            virtual_driver: true,
        });
    }

    Ok(out)
}

/// 当前默认录音设备 ID（三种 role）。
pub fn default_capture_ids() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    // role 数值对应 IPolicyConfig 的三种角色顺序，供恢复默认设备使用。
    for (role_n, role) in [(0u32, eConsole), (1, eMultimedia), (2, eCommunications)] {
        if let Ok(id) = default_endpoint_id(eCapture, role) {
            out.push((role_n, id));
        }
    }
    out
}

/// 当前默认播放端的 console role 设备 ID。
pub fn default_render_id() -> Result<String, Error> {
    default_endpoint_id(eRender, eConsole)
}

/// 枚举阶段收集的端点元数据；描述和适配器字段只用于驱动识别，不作 UI 文案。
#[derive(Clone, Debug)]
struct Endpoint {
    /// IMMDevice 的不透明字符串，用于再次激活同一端点。
    id: String,
    /// FriendlyName，仅用于列表展示和错误提示。
    name: String,
    /// DeviceDesc，参与虚拟驱动与输入/输出配对判定。
    desc: String,
    /// 驱动接口友好名，作为跨渲染/录音端的分组依据。
    adapter: String,
    /// 该端点是否是当前 role 的默认设备，用于稳定排序。
    is_default: bool,
}

/// 将内部端点元数据裁剪为物理麦克风公开结构，避免暴露配对细节。
impl Endpoint {
    /// 消费端点并保留设备 ID 与展示名，其他识别字段不再需要。
    fn into_render(self) -> RenderDevice {
        RenderDevice {
            id: self.id,
            name: self.name,
        }
    }
}

/// 在当前 MTA 中读取活动端点及其属性，并把默认端点排到列表前面。
fn list_endpoints(flow: EDataFlow, default_role: ERole) -> Result<Vec<Endpoint>, Error> {
    // 枚举器和属性存储都由本次 COM 调用借用，返回值只包含已复制的 Rust 字符串。
    let _com = com::init_mta()?;
    // SAFETY: 所有 COM 接口在当前 MTA 中同步创建、使用和释放；返回结构只拥有复制后的字符串。
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let collection = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
        let count = collection.GetCount()?;
        let default_id = enumerator
            .GetDefaultAudioEndpoint(flow, default_role)
            .ok()
            .and_then(|d| device_id(&d).ok());

        let mut devices = Vec::with_capacity(count as usize);
        // 只枚举活动端点；属性缺失时仍保留设备 ID，并将空描述视为未知。
        for i in 0..count {
            let device = collection.Item(i)?;
            let id = device_id(&device)?;
            let name = prop_key(&device, &PKEY_Device_FriendlyName).unwrap_or_else(|| id.clone());
            let desc = prop_key(&device, &PKEY_Device_DeviceDesc).unwrap_or_default();
            let adapter = prop_key(&device, &PKEY_DeviceInterface_FriendlyName).unwrap_or_default();
            let is_default = default_id.as_ref() == Some(&id);
            devices.push(Endpoint {
                id,
                name,
                desc,
                adapter,
                is_default,
            });
        }
        // 默认端点置前供选择器稳定呈现，其余设备按名称排序以避免系统枚举顺序造成列表抖动。
        devices.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
        Ok(devices)
    }
}

/// 查询指定数据流和角色的默认端点；设备 ID 仍按不透明字符串传递。
fn default_endpoint_id(flow: EDataFlow, role: ERole) -> Result<String, Error> {
    let _com = com::init_mta()?;
    // SAFETY: 默认端点接口仅在当前 MTA 同步使用，返回 ID 在函数内复制并释放。
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(flow, role)?;
        device_id(&device)
    }
}

/// 依据驱动家族为录音端寻找对应播放端，避免把用户改名当作配对依据。
fn paired_render<'a>(capture: &Endpoint, renders: &'a [Endpoint]) -> Option<&'a Endpoint> {
    if !is_virtual_driver(capture) {
        return None;
    }
    // 同一驱动可能有多个渲染端，先收集候选再用方向描述消歧。
    let mut same: Vec<&Endpoint> = renders
        .iter()
        .filter(|r| is_virtual_driver(r) && same_driver(capture, r))
        .collect();
    if same.is_empty() {
        return None;
    }
    if same.len() == 1 {
        return Some(same[0]);
    }
    if let Some(r) = same
        .iter()
        .copied()
        .find(|r| complementary(&r.desc, &capture.desc))
    {
        return Some(r);
    }
    // 没有明确方向标记时按名称稳定排序，保证每次枚举选中同一端点。
    same.sort_by(|a, b| a.name.cmp(&b.name));
    Some(same[0])
}
/// 判断两个端点是否由同一适配器和驱动家族提供，防止同时打开互斥端点。
fn same_driver(a: &Endpoint, b: &Endpoint) -> bool {
    if !a.adapter.is_empty() && !b.adapter.is_empty() && a.adapter.eq_ignore_ascii_case(&b.adapter)
    {
        return driver_family(a) == driver_family(b);
    }
    driver_family(a) == driver_family(b)
}

/// 取端点的驱动分组键：用驱动接口名（`PKEY_DeviceInterface_FriendlyName`）。
///
/// 同一虚拟驱动的两个渲染端点共享该名字，实测不能同时打开
/// （VB-Cable 的「CABLE虚拟扬声器」与「CABLE In 16ch」第二个返回
/// `AUDCLNT_E_DEVICE_IN_USE`）；物理设备的接口名互不相同，不会误判。
fn driver_key_of(endpoint: &Endpoint) -> String {
    if endpoint.adapter.is_empty() {
        endpoint.id.clone()
    } else {
        endpoint.adapter.to_ascii_lowercase()
    }
}

/// 通过驱动描述识别虚拟音频端；FriendlyName 可被用户改写，不能作为唯一依据。
fn is_virtual_driver(ep: &Endpoint) -> bool {
    looks_virtual(&ep.adapter) || looks_virtual(&ep.desc)
}

/// 匹配已知虚拟驱动关键词；大小写归一化使不同厂商命名保持一致。
fn looks_virtual(s: &str) -> bool {
    let n = s.to_ascii_uppercase();
    n.contains("VB-AUDIO")
        || n.contains("VB AUDIO")
        || n.contains("VOICEMEETER")
        || n.contains("STEAM STREAMING")
        || n.contains("VIRTUAL AUDIO")
        || n.contains("CABLE")
}

/// 将驱动描述归并到稳定家族名，用于端点互斥和配对回退。
fn driver_family(ep: &Endpoint) -> &'static str {
    let n = format!("{} {}", ep.adapter, ep.desc).to_ascii_uppercase();
    if n.contains("16") && (n.contains("CABLE") || n.contains("VB-AUDIO")) {
        "cable16"
    } else if n.contains("CABLE") || n.contains("VB-AUDIO") {
        "cable"
    } else if n.contains("VOICEMEETER") {
        "voicemeeter"
    } else if n.contains("STEAM STREAMING") {
        "steam"
    } else {
        "other"
    }
}

/// 用方向词判断渲染端与录音端是否是互补的一对。
fn complementary(render_desc: &str, capture_desc: &str) -> bool {
    let r = render_desc.to_ascii_uppercase();
    let c = capture_desc.to_ascii_uppercase();
    let render_in =
        r.contains("INPUT") || r.contains("IN ") || r.contains("输入") || r.contains("SPEAKER");
    let capture_out = c.contains("OUTPUT")
        || c.contains("OUT")
        || c.contains("输出")
        || c.contains("MIC")
        || c.contains("麦克");
    render_in && capture_out
}

/// IMMDevice::GetId，调用方负责理解这是系统设备字符串。
pub fn device_id(device: &IMMDevice) -> Result<String, Error> {
    // SAFETY: GetId 返回的 CoTaskMem 字符串由本函数同步复制并立即释放，不跨线程保存指针。
    unsafe {
        let pwstr = device.GetId()?;
        let id = pwstr.to_string().unwrap_or_default();
        CoTaskMemFree(Some(pwstr.0 as *const _));
        if id.is_empty() {
            Err(Error::DeviceNotFound)
        } else {
            Ok(id)
        }
    }
}

/// 读取端点属性并把 PROPVARIANT 转成拥有所有权的 Rust 字符串。
fn prop_key(device: &IMMDevice, key: &PROPERTYKEY) -> Option<String> {
    // SAFETY: 属性存储和 PROPVARIANT 只在当前 COM 调用中借用，清理在离开前显式完成。
    unsafe {
        let store: IPropertyStore = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut pv = store.GetValue(key).ok()?;
        let name = propvariant_string(&pv);
        let _ = windows::Win32::System::Com::StructuredStorage::PropVariantClear(&mut pv);
        if name.is_empty() { None } else { Some(name) }
    }
}

/// 从 PROPVARIANT 取宽字符串。
/// 根据 PROPVARIANT 类型取出宽字符串；其他类型按缺失属性处理。
fn propvariant_string(pv: &windows::Win32::System::Com::StructuredStorage::PROPVARIANT) -> String {
    // SAFETY: 返回的宽字符串指针属于 PROPVARIANT，在本函数内读取且不转移所有权。
    unsafe {
        let vt = pv.Anonymous.Anonymous.vt;
        if vt == VT_LPWSTR || vt == VT_BSTR {
            let p = pv.Anonymous.Anonymous.Anonymous.pwszVal;
            p.to_string().unwrap_or_default()
        } else {
            String::new()
        }
    }
}
