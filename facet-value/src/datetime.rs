//! DateTime value type for representing temporal data.
//!
//! `VDateTime` supports the four datetime categories from TOML:
//! - Offset Date-Time: `1979-05-27T07:32:00Z` or `1979-05-27T07:32:00+01:30`
//! - Local Date-Time: `1979-05-27T07:32:00`
//! - Local Date: `1979-05-27`
//! - Local Time: `07:32:00`

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc};
use core::cmp::Ordering;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};

use crate::value::{TypeTag, Value};

/// The kind of datetime value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateTimeKind {
    /// Offset date-time with UTC offset in minutes.
    /// e.g., `1979-05-27T07:32:00Z` (offset=0) or `1979-05-27T07:32:00+05:30` (offset=330)
    Offset {
        /// Offset from UTC in minutes. Range: -1440 to +1440 (±24 hours).
        offset_minutes: i16,
    },

    /// Local date-time without offset (civil time).
    /// e.g., `1979-05-27T07:32:00`
    LocalDateTime,

    /// Local date only.
    /// e.g., `1979-05-27`
    LocalDate,

    /// Local time only.
    /// e.g., `07:32:00`
    LocalTime,
}

/// Header for heap-allocated datetime values.
#[repr(C, align(8))]
struct DateTimeHeader {
    /// Year (negative for BCE). For LocalTime, this is 0.
    year: i32,
    /// Month (1-12). For LocalTime, this is 0.
    month: u8,
    /// Day (1-31). For LocalTime, this is 0.
    day: u8,
    /// Hour (0-23). For LocalDate, this is 0.
    hour: u8,
    /// Minute (0-59). For LocalDate, this is 0.
    minute: u8,
    /// Second (0-59, or 60 for leap second). For LocalDate, this is 0.
    second: u8,
    /// Padding for alignment
    _pad: [u8; 3],
    /// Nanoseconds (0-999_999_999). For LocalDate, this is 0.
    nanos: u32,
    /// The kind of datetime
    kind: DateTimeKind,
}

/// A datetime value.
///
/// `VDateTime` can represent offset date-times, local date-times, local dates,
/// or local times. This covers all datetime types in TOML and most other formats.
#[repr(transparent)]
#[derive(Clone)]
pub struct VDateTime(pub(crate) Value);

impl VDateTime {
    const fn layout() -> Layout {
        Layout::new::<DateTimeHeader>()
    }

    #[cfg(feature = "alloc")]
    fn alloc() -> *mut DateTimeHeader {
        unsafe { alloc(Self::layout()).cast::<DateTimeHeader>() }
    }

    #[cfg(feature = "alloc")]
    fn dealloc(ptr: *mut DateTimeHeader) {
        unsafe {
            dealloc(ptr.cast::<u8>(), Self::layout());
        }
    }

    fn header(&self) -> &DateTimeHeader {
        unsafe { &*(self.0.heap_ptr() as *const DateTimeHeader) }
    }

    #[allow(dead_code)]
    fn header_mut(&mut self) -> &mut DateTimeHeader {
        unsafe { &mut *(self.0.heap_ptr_mut() as *mut DateTimeHeader) }
    }

    /// Creates a new offset date-time.
    ///
    /// # Arguments
    /// * `year` - Year (negative for BCE)
    /// * `month` - Month (1-12)
    /// * `day` - Day (1-31)
    /// * `hour` - Hour (0-23)
    /// * `minute` - Minute (0-59)
    /// * `second` - Second (0-59, or 60 for leap second)
    /// * `nanos` - Nanoseconds (0-999_999_999)
    /// * `offset_minutes` - Offset from UTC in minutes
    #[cfg(feature = "alloc")]
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new_offset(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
        offset_minutes: i16,
    ) -> Self {
        unsafe {
            let ptr = Self::alloc();
            (*ptr).year = year;
            (*ptr).month = month;
            (*ptr).day = day;
            (*ptr).hour = hour;
            (*ptr).minute = minute;
            (*ptr).second = second;
            (*ptr)._pad = [0; 3];
            (*ptr).nanos = nanos;
            (*ptr).kind = DateTimeKind::Offset { offset_minutes };
            VDateTime(Value::new_ptr(ptr.cast(), TypeTag::DateTime))
        }
    }

    /// Creates a new local date-time (no offset).
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new_local_datetime(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
    ) -> Self {
        unsafe {
            let ptr = Self::alloc();
            (*ptr).year = year;
            (*ptr).month = month;
            (*ptr).day = day;
            (*ptr).hour = hour;
            (*ptr).minute = minute;
            (*ptr).second = second;
            (*ptr)._pad = [0; 3];
            (*ptr).nanos = nanos;
            (*ptr).kind = DateTimeKind::LocalDateTime;
            VDateTime(Value::new_ptr(ptr.cast(), TypeTag::DateTime))
        }
    }

    /// Creates a new local date (no time component).
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new_local_date(year: i32, month: u8, day: u8) -> Self {
        unsafe {
            let ptr = Self::alloc();
            (*ptr).year = year;
            (*ptr).month = month;
            (*ptr).day = day;
            (*ptr).hour = 0;
            (*ptr).minute = 0;
            (*ptr).second = 0;
            (*ptr)._pad = [0; 3];
            (*ptr).nanos = 0;
            (*ptr).kind = DateTimeKind::LocalDate;
            VDateTime(Value::new_ptr(ptr.cast(), TypeTag::DateTime))
        }
    }

    /// Creates a new local time (no date component).
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new_local_time(hour: u8, minute: u8, second: u8, nanos: u32) -> Self {
        unsafe {
            let ptr = Self::alloc();
            (*ptr).year = 0;
            (*ptr).month = 0;
            (*ptr).day = 0;
            (*ptr).hour = hour;
            (*ptr).minute = minute;
            (*ptr).second = second;
            (*ptr)._pad = [0; 3];
            (*ptr).nanos = nanos;
            (*ptr).kind = DateTimeKind::LocalTime;
            VDateTime(Value::new_ptr(ptr.cast(), TypeTag::DateTime))
        }
    }

    /// Returns the kind of datetime.
    #[must_use]
    pub fn kind(&self) -> DateTimeKind {
        self.header().kind
    }

    /// Returns the year. Returns 0 for LocalTime.
    #[must_use]
    pub fn year(&self) -> i32 {
        self.header().year
    }

    /// Returns the month (1-12). Returns 0 for LocalTime.
    #[must_use]
    pub fn month(&self) -> u8 {
        self.header().month
    }

    /// Returns the day (1-31). Returns 0 for LocalTime.
    #[must_use]
    pub fn day(&self) -> u8 {
        self.header().day
    }

    /// Returns the hour (0-23). Returns 0 for LocalDate.
    #[must_use]
    pub fn hour(&self) -> u8 {
        self.header().hour
    }

    /// Returns the minute (0-59). Returns 0 for LocalDate.
    #[must_use]
    pub fn minute(&self) -> u8 {
        self.header().minute
    }

    /// Returns the second (0-59, or 60 for leap second). Returns 0 for LocalDate.
    #[must_use]
    pub fn second(&self) -> u8 {
        self.header().second
    }

    /// Returns the nanoseconds (0-999_999_999). Returns 0 for LocalDate.
    #[must_use]
    pub fn nanos(&self) -> u32 {
        self.header().nanos
    }

    /// Returns the UTC offset in minutes, if this is an offset datetime.
    #[must_use]
    pub fn offset_minutes(&self) -> Option<i16> {
        match self.kind() {
            DateTimeKind::Offset { offset_minutes } => Some(offset_minutes),
            _ => None,
        }
    }

    /// Returns true if this datetime has a date component.
    #[must_use]
    pub fn has_date(&self) -> bool {
        !matches!(self.kind(), DateTimeKind::LocalTime)
    }

    /// Returns true if this datetime has a time component.
    #[must_use]
    pub fn has_time(&self) -> bool {
        !matches!(self.kind(), DateTimeKind::LocalDate)
    }

    /// Returns true if this datetime has an offset.
    #[must_use]
    pub fn has_offset(&self) -> bool {
        matches!(self.kind(), DateTimeKind::Offset { .. })
    }

    // === Internal ===

    pub(crate) fn clone_impl(&self) -> Value {
        #[cfg(feature = "alloc")]
        {
            let h = self.header();
            match h.kind {
                DateTimeKind::Offset { offset_minutes } => {
                    Self::new_offset(
                        h.year,
                        h.month,
                        h.day,
                        h.hour,
                        h.minute,
                        h.second,
                        h.nanos,
                        offset_minutes,
                    )
                    .0
                }
                DateTimeKind::LocalDateTime => {
                    Self::new_local_datetime(
                        h.year, h.month, h.day, h.hour, h.minute, h.second, h.nanos,
                    )
                    .0
                }
                DateTimeKind::LocalDate => Self::new_local_date(h.year, h.month, h.day).0,
                DateTimeKind::LocalTime => {
                    Self::new_local_time(h.hour, h.minute, h.second, h.nanos).0
                }
            }
        }
        #[cfg(not(feature = "alloc"))]
        {
            panic!("cannot clone VDateTime without alloc feature")
        }
    }

    pub(crate) fn drop_impl(&mut self) {
        #[cfg(feature = "alloc")]
        unsafe {
            Self::dealloc(self.0.heap_ptr_mut().cast());
        }
    }
}

// === PartialEq, Eq ===

impl PartialEq for VDateTime {
    fn eq(&self, other: &Self) -> bool {
        let (h1, h2) = (self.header(), other.header());
        h1.kind == h2.kind
            && h1.year == h2.year
            && h1.month == h2.month
            && h1.day == h2.day
            && h1.hour == h2.hour
            && h1.minute == h2.minute
            && h1.second == h2.second
            && h1.nanos == h2.nanos
    }
}

impl Eq for VDateTime {}

// === PartialOrd ===

impl PartialOrd for VDateTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let (h1, h2) = (self.header(), other.header());

        // Only compare within the same kind
        match (&h1.kind, &h2.kind) {
            (
                DateTimeKind::Offset { offset_minutes: o1 },
                DateTimeKind::Offset { offset_minutes: o2 },
            ) => {
                // Convert to comparable instant (seconds from epoch-ish)
                // We don't need actual epoch, just consistent comparison
                let to_comparable = |h: &DateTimeHeader, offset: i16| -> (i64, u32) {
                    let days = h.year as i64 * 366 + h.month as i64 * 31 + h.day as i64;
                    let secs = days * 86400
                        + h.hour as i64 * 3600
                        + h.minute as i64 * 60
                        + h.second as i64
                        - offset as i64 * 60;
                    (secs, h.nanos)
                };
                let c1 = to_comparable(h1, *o1);
                let c2 = to_comparable(h2, *o2);
                c1.partial_cmp(&c2)
            }
            (DateTimeKind::LocalDateTime, DateTimeKind::LocalDateTime)
            | (DateTimeKind::LocalDate, DateTimeKind::LocalDate) => {
                // Lexicographic comparison
                (
                    h1.year, h1.month, h1.day, h1.hour, h1.minute, h1.second, h1.nanos,
                )
                    .partial_cmp(&(
                        h2.year, h2.month, h2.day, h2.hour, h2.minute, h2.second, h2.nanos,
                    ))
            }
            (DateTimeKind::LocalTime, DateTimeKind::LocalTime) => {
                (h1.hour, h1.minute, h1.second, h1.nanos)
                    .partial_cmp(&(h2.hour, h2.minute, h2.second, h2.nanos))
            }
            _ => None, // Different kinds are not comparable
        }
    }
}

// === Hash ===

impl Hash for VDateTime {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let h = self.header();
        h.kind.hash(state);
        h.year.hash(state);
        h.month.hash(state);
        h.day.hash(state);
        h.hour.hash(state);
        h.minute.hash(state);
        h.second.hash(state);
        h.nanos.hash(state);
    }
}

// === Debug ===

impl Debug for VDateTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let h = self.header();
        match h.kind {
            DateTimeKind::Offset { offset_minutes } => {
                write!(
                    f,
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                    h.year, h.month, h.day, h.hour, h.minute, h.second
                )?;
                if h.nanos > 0 {
                    write!(f, ".{:09}", h.nanos)?;
                }
                if offset_minutes == 0 {
                    write!(f, "Z")
                } else {
                    let sign = if offset_minutes >= 0 { '+' } else { '-' };
                    let abs = offset_minutes.abs();
                    write!(f, "{}{:02}:{:02}", sign, abs / 60, abs % 60)
                }
            }
            DateTimeKind::LocalDateTime => {
                write!(
                    f,
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                    h.year, h.month, h.day, h.hour, h.minute, h.second
                )?;
                if h.nanos > 0 {
                    write!(f, ".{:09}", h.nanos)?;
                }
                Ok(())
            }
            DateTimeKind::LocalDate => {
                write!(f, "{:04}-{:02}-{:02}", h.year, h.month, h.day)
            }
            DateTimeKind::LocalTime => {
                write!(f, "{:02}:{:02}:{:02}", h.hour, h.minute, h.second)?;
                if h.nanos > 0 {
                    write!(f, ".{:09}", h.nanos)?;
                }
                Ok(())
            }
        }
    }
}

// === From ===

#[cfg(feature = "alloc")]
impl From<VDateTime> for Value {
    fn from(dt: VDateTime) -> Self {
        dt.0
    }
}
