//! `DataFixers.addFixers`, versions 1460..2199: the schemas (the fixes are registered by a later change).

use crate::fixer::DataFixerBuilder;
use crate::schema::Schema;
use crate::schemas::{
    v1460, v1466, v1470, v1481, v1483, v1486, v1488, v1510, v1800, v1801, v1904, v1906, v1909,
    v1920, v1928, v1929, v1931, v2100,
};

pub fn add(b: &mut DataFixerBuilder) {
    b.add_schema(1460, v1460::new);
    b.add_schema(1466, v1466::new);
    b.add_schema(1470, v1470::new);
    b.add_schema(1474, Schema::same_namespaced);
    b.add_schema(1475, Schema::same_namespaced);
    b.add_schema(1480, Schema::same_namespaced);
    b.add_schema(1481, v1481::new);
    b.add_schema(1483, v1483::new);
    b.add_schema(1484, Schema::same_namespaced);
    b.add_schema(1486, v1486::new);
    b.add_schema(1487, Schema::same_namespaced);
    b.add_schema(1488, v1488::new);
    b.add_schema(1490, Schema::same_namespaced);
    b.add_schema(1492, Schema::same_namespaced);
    b.add_schema(1494, Schema::same_namespaced);
    b.add_schema(1496, Schema::same_namespaced);
    b.add_schema(1500, Schema::same_namespaced);
    b.add_schema(1501, Schema::same_namespaced);
    b.add_schema(1502, Schema::same_namespaced);
    b.add_schema(1506, Schema::same_namespaced);
    b.add_schema(1510, v1510::new);
    b.add_schema(1514, Schema::same_namespaced);
    b.add_schema(1515, Schema::same_namespaced);
    b.add_schema(1624, Schema::same_namespaced);
    b.add_schema(1800, v1800::new);
    b.add_schema(1801, v1801::new);
    b.add_schema(1802, Schema::same_namespaced);
    b.add_schema(1803, Schema::same_namespaced);
    b.add_schema(1904, v1904::new);
    b.add_schema(1905, Schema::same_namespaced);
    b.add_schema(1906, v1906::new);
    b.add_schema(1909, v1909::new);
    b.add_schema(1911, Schema::same_namespaced);
    b.add_schema(1914, Schema::same_namespaced);
    b.add_schema(1917, Schema::same_namespaced);
    b.add_schema(1918, Schema::same_namespaced);
    b.add_schema(1920, v1920::new);
    b.add_schema(1925, Schema::same_namespaced);
    b.add_schema(1928, v1928::new);
    b.add_schema(1929, v1929::new);
    b.add_schema(1931, v1931::new);
    b.add_schema(1936, Schema::same_namespaced);
    b.add_schema(1946, Schema::same_namespaced);
    b.add_schema(1948, Schema::same_namespaced);
    b.add_schema(1953, Schema::same_namespaced);
    b.add_schema(1955, Schema::same_namespaced);
    b.add_schema(1961, Schema::same_namespaced);
    b.add_schema(1963, Schema::same_namespaced);
    b.add_schema(2100, v2100::new);
}
