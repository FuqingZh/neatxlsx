use std::path::PathBuf;
use std::sync::Arc;

use arrow::array::StructArray;
use arrow::datatypes::{ArrowDataType, ArrowSchema, Field as ArrowField};
use arrow::record_batch::RecordBatchT;
use neatxlsx_core::constant::{ColumnIdentifier, create_default_xlsx_write_options};
use neatxlsx_core::spec::{
    AutofitMode, AutofitPolicy, CellFormatPatch, ColumnValueKind, ColumnValuePlan,
    IntegerCoerceMode, ScientificPolicy, ScientificScope, SheetSlice, XlsxValuePolicy,
    XlsxWriteOptions,
};
use neatxlsx_core::{
    XlsxRecordBatch, XlsxRecordBatchResult, XlsxSheetWriteOptions, XlsxWriter as RsXlsxWriter,
};
use polars::prelude::{AnyValue, DataFrame};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::ffi as pyffi;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyIterator, PyList, PyTuple};

pub const BRIDGE_ABI_VERSION: u64 = 3;
pub const BRIDGE_CONTRACT_VERSION: &str = "neatxlsx.xlsx.writer.v3";
pub const BRIDGE_TRANSPORT: &str = "arrow_c_data";
pub const BUILD_PROFILE: &str = env!("NEATXLSX_BUILD_PROFILE");
const C_ARROW_ARRAY_STREAM_CAPSULE_NAME: &[u8] = b"arrow_array_stream\0";
const PY_CLASS_SHEET_SLICE: &str = "SheetSlice";
const PY_CLASS_XLSX_REPORT: &str = "XlsxReport";
const PY_CLASS_XLSX_ARROW_DRAIN_PROFILE: &str = "XlsxArrowDrainProfile";
const PY_ARG_OPTIONS_WRITE: &str = "options_write";
const PY_ARG_POLICY_AUTOFIT: &str = "policy_autofit";
const PY_ARG_POLICY_SCIENTIFIC: &str = "policy_scientific";
const PY_VISIBLE_SYMBOLS: [&str; 6] = [
    PY_CLASS_SHEET_SLICE,
    PY_CLASS_XLSX_REPORT,
    PY_CLASS_XLSX_ARROW_DRAIN_PROFILE,
    PY_ARG_OPTIONS_WRITE,
    PY_ARG_POLICY_AUTOFIT,
    PY_ARG_POLICY_SCIENTIFIC,
];

#[pyclass(name = "XlsxArrowDrainProfile")]
#[derive(Debug, Clone)]
struct PyXlsxArrowDrainProfile {
    #[pyo3(get)]
    batches: usize,
    #[pyo3(get)]
    rows: usize,
    #[pyo3(get)]
    cols: usize,
    #[pyo3(get)]
    cells: usize,
}

#[pyclass(name = "XlsxWriter")]
struct PyXlsxWriter {
    #[pyo3(get)]
    file_out: String,
    inner: RsXlsxWriter,
}

#[pymethods]
impl PyXlsxWriter {
    #[new]
    #[pyo3(signature = (
        file_out,
        fmt_text = None,
        fmt_integer = None,
        fmt_decimal = None,
        fmt_scientific = None,
        fmt_header = None,
        options_write = None
    ))]
    fn new(
        file_out: String,
        fmt_text: Option<&Bound<'_, PyAny>>,
        fmt_integer: Option<&Bound<'_, PyAny>>,
        fmt_decimal: Option<&Bound<'_, PyAny>>,
        fmt_scientific: Option<&Bound<'_, PyAny>>,
        fmt_header: Option<&Bound<'_, PyAny>>,
        options_write: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let path_file_out = PathBuf::from(&file_out);

        let c_fmt_text = parse_cell_format_patch(fmt_text)?.unwrap_or_default();
        let c_fmt_integer = parse_cell_format_patch(fmt_integer)?.unwrap_or_default();
        let c_fmt_decimal = parse_cell_format_patch(fmt_decimal)?.unwrap_or_default();
        let c_fmt_scientific = parse_cell_format_patch(fmt_scientific)?.unwrap_or_default();
        let c_fmt_header = parse_cell_format_patch(fmt_header)?.unwrap_or_default();

        let cfg_options_write = parse_xlsx_write_options(options_write)?
            .unwrap_or_else(create_default_xlsx_write_options);

        let inner = RsXlsxWriter::new(
            path_file_out,
            c_fmt_text,
            c_fmt_integer,
            c_fmt_decimal,
            c_fmt_scientific,
            c_fmt_header,
            cfg_options_write,
        )
        .map_err(PyValueError::new_err)?;

        Ok(Self { file_out, inner })
    }

    fn __enter__(slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc=None, _tb=None))]
    fn __exit__(
        &mut self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc: Option<&Bound<'_, PyAny>>,
        _tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        self.close()
    }

    fn close(&mut self) -> PyResult<()> {
        self.inner.close().map_err(PyRuntimeError::new_err)
    }

    fn report(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l_reports = self.inner.report();

        let module_spec = py.import("neatxlsx.spec")?;
        let cls_sheet_slice = module_spec.getattr(PY_CLASS_SHEET_SLICE)?;
        let cls_xlsx_report = module_spec.getattr(PY_CLASS_XLSX_REPORT)?;

        let mut l_report_obj = Vec::with_capacity(l_reports.len());
        for report in l_reports {
            let mut l_sheet_obj = Vec::with_capacity(report.sheets.len());
            for sheet in report.sheets {
                l_sheet_obj.push(create_sheet_slice_object(&cls_sheet_slice, &sheet)?);
            }

            let inst_report =
                cls_xlsx_report.call1((PyList::new(py, l_sheet_obj)?, report.warnings))?;
            l_report_obj.push(inst_report.unbind());
        }

        let tup_report = PyTuple::new(py, l_report_obj)?;
        Ok(tup_report.into_any().unbind())
    }

    #[pyo3(signature = (
        body,
        sheet_name,
        header = None,
        cols_integer = None,
        cols_decimal = None,
        num_frozen_cols = 0,
        num_frozen_rows = None,
        should_merge_header = false,
        should_keep_missing_values = None,
        policy_autofit = None,
        policy_scientific = None,
        value_plans = None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn write_sheet<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
        body: &Bound<'py, PyAny>,
        sheet_name: &str,
        header: Option<&Bound<'py, PyAny>>,
        cols_integer: Option<&Bound<'py, PyAny>>,
        cols_decimal: Option<&Bound<'py, PyAny>>,
        num_frozen_cols: usize,
        num_frozen_rows: Option<usize>,
        should_merge_header: bool,
        should_keep_missing_values: Option<bool>,
        policy_autofit: Option<&Bound<'py, PyAny>>,
        policy_scientific: Option<&Bound<'py, PyAny>>,
        value_plans: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let cfg_sheet_write_options = XlsxSheetWriteOptions {
            cols_integer: parse_column_refs(cols_integer)?,
            cols_decimal: parse_column_refs(cols_decimal)?,
            num_frozen_cols,
            num_frozen_rows,
            should_merge_header,
            should_keep_missing_values,
            policy_autofit: parse_autofit_policy(policy_autofit)?
                .unwrap_or_else(AutofitPolicy::default),
            policy_scientific: parse_scientific_policy(policy_scientific)?
                .unwrap_or_else(ScientificPolicy::default),
            value_plans: parse_column_value_plans(value_plans)?,
        };

        let header_grid = derive_optional_header_grid(py, header)?;
        let plan = slf
            .inner
            .plan_sheet_from_record_batch_results(
                PyRecordBatchIter::from_dataframe(py, body),
                sheet_name,
                header_grid,
                &cfg_sheet_write_options,
            )
            .map_err(PyValueError::new_err)?;
        slf.inner
            .write_sheet_from_record_batch_results(
                plan,
                PyRecordBatchIter::from_dataframe(py, body),
                &cfg_sheet_write_options,
            )
            .map_err(PyValueError::new_err)?;

        Ok(slf)
    }

    #[pyo3(signature = (
        batches_scan,
        batches_write,
        sheet_name,
        header = None,
        cols_integer = None,
        cols_decimal = None,
        num_frozen_cols = 0,
        num_frozen_rows = None,
        should_merge_header = false,
        should_keep_missing_values = None,
        policy_autofit = None,
        policy_scientific = None,
        schema_body = None,
        value_plans = None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn write_sheet_batches<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
        batches_scan: &Bound<'py, PyAny>,
        batches_write: &Bound<'py, PyAny>,
        sheet_name: &str,
        header: Option<&Bound<'py, PyAny>>,
        cols_integer: Option<&Bound<'py, PyAny>>,
        cols_decimal: Option<&Bound<'py, PyAny>>,
        num_frozen_cols: usize,
        num_frozen_rows: Option<usize>,
        should_merge_header: bool,
        should_keep_missing_values: Option<bool>,
        policy_autofit: Option<&Bound<'py, PyAny>>,
        policy_scientific: Option<&Bound<'py, PyAny>>,
        schema_body: Option<&Bound<'py, PyAny>>,
        value_plans: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let cfg_sheet_write_options = XlsxSheetWriteOptions {
            cols_integer: parse_column_refs(cols_integer)?,
            cols_decimal: parse_column_refs(cols_decimal)?,
            num_frozen_cols,
            num_frozen_rows,
            should_merge_header,
            should_keep_missing_values,
            policy_autofit: parse_autofit_policy(policy_autofit)?
                .unwrap_or_else(AutofitPolicy::default),
            policy_scientific: parse_scientific_policy(policy_scientific)?
                .unwrap_or_else(ScientificPolicy::default),
            value_plans: parse_column_value_plans(value_plans)?,
        };

        let header_grid = derive_optional_header_grid(py, header)?;
        let plan = slf
            .inner
            .plan_sheet_from_record_batch_results(
                PyRecordBatchIter::from_arrow_stream_or_iterable_with_schema(
                    batches_scan,
                    schema_body,
                )?,
                sheet_name,
                header_grid,
                &cfg_sheet_write_options,
            )
            .map_err(PyValueError::new_err)?;
        slf.inner
            .write_sheet_from_record_batch_results(
                plan,
                PyRecordBatchIter::from_arrow_stream_or_iterable_with_schema(
                    batches_write,
                    schema_body,
                )?,
                &cfg_sheet_write_options,
            )
            .map_err(PyValueError::new_err)?;

        Ok(slf)
    }

    #[pyo3(signature = (
        batches_write,
        sheet_name,
        header = None,
        cols_integer = None,
        cols_decimal = None,
        num_frozen_cols = 0,
        num_frozen_rows = None,
        should_merge_header = false,
        should_keep_missing_values = None,
        policy_autofit = None,
        policy_scientific = None,
        schema_body = None,
        value_plans = None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn write_sheet_batches_single_pass<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
        batches_write: &Bound<'py, PyAny>,
        sheet_name: &str,
        header: Option<&Bound<'py, PyAny>>,
        cols_integer: Option<&Bound<'py, PyAny>>,
        cols_decimal: Option<&Bound<'py, PyAny>>,
        num_frozen_cols: usize,
        num_frozen_rows: Option<usize>,
        should_merge_header: bool,
        should_keep_missing_values: Option<bool>,
        policy_autofit: Option<&Bound<'py, PyAny>>,
        policy_scientific: Option<&Bound<'py, PyAny>>,
        schema_body: Option<&Bound<'py, PyAny>>,
        value_plans: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let cfg_sheet_write_options = XlsxSheetWriteOptions {
            cols_integer: parse_column_refs(cols_integer)?,
            cols_decimal: parse_column_refs(cols_decimal)?,
            num_frozen_cols,
            num_frozen_rows,
            should_merge_header,
            should_keep_missing_values,
            policy_autofit: parse_autofit_policy(policy_autofit)?
                .unwrap_or_else(AutofitPolicy::default),
            policy_scientific: parse_scientific_policy(policy_scientific)?
                .unwrap_or_else(ScientificPolicy::default),
            value_plans: parse_column_value_plans(value_plans)?,
        };

        let header_grid = derive_optional_header_grid(py, header)?;
        slf.inner
            .write_sheet_from_record_batch_results_single_pass(
                PyRecordBatchIter::from_arrow_stream_or_iterable_with_schema(
                    batches_write,
                    schema_body,
                )?,
                sheet_name,
                header_grid,
                &cfg_sheet_write_options,
            )
            .map_err(PyValueError::new_err)?;

        Ok(slf)
    }
}

fn create_sheet_slice_object(
    cls_spec_sheet_slice: &Bound<'_, PyAny>,
    sheet: &SheetSlice,
) -> PyResult<Py<PyAny>> {
    let inst_sheet = cls_spec_sheet_slice.call1((
        sheet.sheet_name.clone(),
        sheet.row_start_inclusive,
        sheet.row_end_exclusive,
        sheet.col_start_inclusive,
        sheet.col_end_exclusive,
    ))?;
    Ok(inst_sheet.into_any().unbind())
}

fn parse_column_value_plans(value: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<ColumnValuePlan>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_none() {
        return Ok(Vec::new());
    }
    let mut plans = Vec::new();
    for item in value.try_iter()? {
        let item = item?;
        let tuple = item.downcast::<PyTuple>()?;
        if tuple.len() != 6 {
            return Err(PyValueError::new_err(
                "value_plans entries must contain six fields.",
            ));
        }
        let name = tuple.get_item(0)?.extract::<String>()?;
        let kind_name = tuple.get_item(1)?.extract::<String>()?;
        let kind = match kind_name.as_str() {
            "null" => ColumnValueKind::Null,
            "boolean" => ColumnValueKind::Boolean,
            "integer" => ColumnValueKind::Integer,
            "float" => ColumnValueKind::Float,
            "decimal" => ColumnValueKind::Decimal,
            "string" => ColumnValueKind::String,
            "date" => ColumnValueKind::Date,
            "datetime" => ColumnValueKind::Datetime,
            "time" => ColumnValueKind::Time,
            "duration" => ColumnValueKind::Duration,
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unknown value plan kind: {kind_name}"
                )));
            }
        };
        let optional_string = |index: usize| -> PyResult<Option<String>> {
            let item = tuple.get_item(index)?;
            if item.is_none() {
                Ok(None)
            } else {
                Ok(Some(item.extract::<String>()?))
            }
        };
        let optional_usize = |index: usize| -> PyResult<Option<usize>> {
            let item = tuple.get_item(index)?;
            if item.is_none() {
                Ok(None)
            } else {
                Ok(Some(item.extract::<usize>()?))
            }
        };
        plans.push(ColumnValuePlan {
            name,
            kind,
            unit: optional_string(2)?,
            timezone: optional_string(3)?,
            decimal_precision: optional_usize(4)?,
            decimal_scale: optional_usize(5)?,
        });
    }
    Ok(plans)
}

struct PyRecordBatchIter<'py> {
    py: Python<'py>,
    iter: Option<Bound<'py, PyIterator>>,
    single: Option<Bound<'py, PyAny>>,
    stream_current: Option<PyArrowCStreamBatchIter<'py>>,
    schema_fallback: Option<Bound<'py, PyAny>>,
    has_yielded_batch: bool,
    has_used_schema_fallback: bool,
    is_done: bool,
}

impl<'py> PyRecordBatchIter<'py> {
    fn from_dataframe(py: Python<'py>, df: &Bound<'py, PyAny>) -> Self {
        Self {
            py,
            iter: None,
            single: Some(df.clone()),
            stream_current: None,
            schema_fallback: None,
            has_yielded_batch: false,
            has_used_schema_fallback: false,
            is_done: false,
        }
    }

    fn from_iterable(obj: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(Self {
            py: obj.py(),
            iter: Some(obj.try_iter()?),
            single: None,
            stream_current: None,
            schema_fallback: None,
            has_yielded_batch: false,
            has_used_schema_fallback: false,
            is_done: false,
        })
    }

    fn from_arrow_stream_or_iterable(obj: &Bound<'py, PyAny>) -> PyResult<Self> {
        Self::from_arrow_stream_or_iterable_with_schema(obj, None)
    }

    fn from_arrow_stream_or_iterable_with_schema(
        obj: &Bound<'py, PyAny>,
        schema_fallback: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        if obj.hasattr("__arrow_c_stream__")? {
            return Ok(Self {
                py: obj.py(),
                iter: None,
                single: None,
                stream_current: Some(create_arrow_c_stream_batch_iter_from_arrow_stream_source(
                    obj,
                )?),
                schema_fallback: schema_fallback.cloned(),
                has_yielded_batch: false,
                has_used_schema_fallback: false,
                is_done: false,
            });
        }

        let mut iter = Self::from_iterable(obj)?;
        iter.schema_fallback = schema_fallback.cloned();
        Ok(iter)
    }
}

impl Iterator for PyRecordBatchIter<'_> {
    type Item = XlsxRecordBatchResult;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(stream_current) = self.stream_current.as_mut() {
                match stream_current.next_batch() {
                    Some(Ok(batch)) => {
                        self.has_yielded_batch = true;
                        return Some(Ok(batch));
                    }
                    Some(Err(err)) => return Some(Err(err.to_string())),
                    None => {
                        self.stream_current = None;
                    }
                }
            }
            if self.is_done {
                return None;
            }

            let item = if let Some(single) = self.single.take() {
                single
            } else if let Some(iter) = self.iter.as_mut() {
                match iter.next() {
                    Some(Ok(item)) => item,
                    Some(Err(err)) => return Some(Err(err.to_string())),
                    None => {
                        if !self.has_yielded_batch && !self.has_used_schema_fallback {
                            if let Some(schema_fallback) = self.schema_fallback.take() {
                                self.has_used_schema_fallback = true;
                                schema_fallback
                            } else {
                                self.is_done = true;
                                return None;
                            }
                        } else {
                            self.is_done = true;
                            return None;
                        }
                    }
                }
            } else if !self.has_yielded_batch && !self.has_used_schema_fallback {
                if let Some(schema_fallback) = self.schema_fallback.take() {
                    self.has_used_schema_fallback = true;
                    schema_fallback
                } else {
                    self.is_done = true;
                    return None;
                }
            } else {
                self.is_done = true;
                return None;
            };

            match create_arrow_c_stream_batch_iter_from_any_dataframe(self.py, &item) {
                Ok(stream_current) => {
                    self.stream_current = Some(stream_current);
                }
                Err(err) => return Some(Err(err.to_string())),
            }
        }
    }
}

struct PyArrowCStreamBatchIter<'py> {
    reader: arrow::ffi::ArrowArrayStreamReader<&'py mut arrow::ffi::ArrowArrayStream>,
    schema_ref: Arc<ArrowSchema>,
    _capsule: Bound<'py, PyAny>,
    _source: Option<Bound<'py, PyAny>>,
}

impl<'py> PyArrowCStreamBatchIter<'py> {
    fn try_new(
        obj_capsule: Bound<'py, PyAny>,
        source: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        let ptr_capsule = obj_capsule.as_ptr();
        let ptr_stream_name = C_ARROW_ARRAY_STREAM_CAPSULE_NAME
            .as_ptr()
            .cast::<std::os::raw::c_char>();

        // Safety: We only pass pointers owned by the Python object for validation.
        let is_capsule_valid = unsafe { pyffi::PyCapsule_IsValid(ptr_capsule, ptr_stream_name) };
        if is_capsule_valid == 0 {
            return Err(PyValueError::new_err(
                "Expected a valid `arrow_array_stream` PyCapsule.",
            ));
        }

        // Safety: Capsule name was validated as `arrow_array_stream` above.
        let ptr_stream = unsafe { pyffi::PyCapsule_GetPointer(ptr_capsule, ptr_stream_name) };
        if ptr_stream.is_null() {
            return Err(PyValueError::new_err(
                "Arrow C stream capsule pointer is null.",
            ));
        }

        let stream = ptr_stream.cast::<arrow::ffi::ArrowArrayStream>();
        let stream_ref: &'py mut arrow::ffi::ArrowArrayStream = unsafe { &mut *stream };
        // Safety: `stream_ref` points to a live ArrowArrayStream owned by the capsule.
        let reader =
            unsafe { arrow::ffi::ArrowArrayStreamReader::try_new(stream_ref) }.map_err(|err| {
                PyValueError::new_err(format!("Failed to open Arrow C stream: {err}"))
            })?;

        let schema_arrow = derive_arrow_schema_from_stream_field(reader.field())?;
        let schema_ref = Arc::new(schema_arrow);

        Ok(Self {
            reader,
            schema_ref,
            _capsule: obj_capsule,
            _source: source,
        })
    }

    fn next_batch(&mut self) -> Option<PyResult<XlsxRecordBatch>> {
        let res_array = unsafe { self.reader.next()? };
        let array_row_batch = match res_array {
            Ok(array) => array,
            Err(err) => {
                return Some(Err(PyValueError::new_err(format!(
                    "Failed to read Arrow stream batch: {err}"
                ))));
            }
        };

        let array_struct = match array_row_batch.as_any().downcast_ref::<StructArray>() {
            Some(array) => array,
            None => {
                return Some(Err(PyValueError::new_err(
                    "Arrow C stream must yield StructArray batches for DataFrame import.",
                )));
            }
        };

        let l_arrays = array_struct.values().to_vec();
        Some(
            RecordBatchT::try_new(array_struct.len(), self.schema_ref.clone(), l_arrays).map_err(
                |err| {
                    PyValueError::new_err(format!(
                        "Failed to construct Arrow record batch from stream: {err}"
                    ))
                },
            ),
        )
    }
}

fn create_arrow_c_stream_batch_iter_from_any_dataframe<'py>(
    py: Python<'py>,
    df: &Bound<'py, PyAny>,
) -> PyResult<PyArrowCStreamBatchIter<'py>> {
    let df_polars = convert_to_polars_dataframe(py, df)?;
    let obj_capsule = df_polars.call_method0("__arrow_c_stream__")?;
    PyArrowCStreamBatchIter::try_new(obj_capsule, Some(df_polars))
}

fn create_arrow_c_stream_batch_iter_from_arrow_stream_source<'py>(
    obj: &Bound<'py, PyAny>,
) -> PyResult<PyArrowCStreamBatchIter<'py>> {
    let obj_capsule = obj.call_method0("__arrow_c_stream__")?;
    PyArrowCStreamBatchIter::try_new(obj_capsule, Some(obj.clone()))
}

fn derive_arrow_schema_from_stream_field(field: &ArrowField) -> PyResult<ArrowSchema> {
    match field.dtype() {
        ArrowDataType::Struct(fields) => Ok(fields
            .iter()
            .cloned()
            .map(|field_inner| (field_inner.name.clone(), field_inner))
            .collect::<ArrowSchema>()),
        dtype => Err(PyValueError::new_err(format!(
            "Arrow stream schema must be Struct, got: {dtype:?}"
        ))),
    }
}

fn derive_optional_header_grid(
    py: Python<'_>,
    header: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Vec<Vec<String>>>> {
    let Some(header_raw) = header else {
        return Ok(None);
    };
    if header_raw.is_none() {
        return Ok(None);
    }

    let df_header = derive_dataframe_from_any_dataframe(py, header_raw)?;
    Ok(Some(extract_string_grid_from_dataframe(&df_header)?))
}

fn derive_dataframe_from_any_dataframe(
    py: Python<'_>,
    df: &Bound<'_, PyAny>,
) -> PyResult<DataFrame> {
    let mut iter_stream = create_arrow_c_stream_batch_iter_from_any_dataframe(py, df)?;
    let mut df_out: Option<DataFrame> = None;

    while let Some(batch) = iter_stream.next_batch() {
        let batch = batch?;
        let schema_arrow = batch.schema().clone();
        let df = df_out.get_or_insert_with(|| DataFrame::empty_with_arrow_schema(&schema_arrow));
        arrow::array::TryExtend::try_extend(df, std::iter::once(batch)).map_err(|err| {
            PyValueError::new_err(format!(
                "Failed to append Arrow record batch to DataFrame: {err}"
            ))
        })?;
    }

    df_out.ok_or_else(|| PyValueError::new_err("Arrow stream did not yield any record batches."))
}

fn extract_string_grid_from_dataframe(df: &DataFrame) -> PyResult<Vec<Vec<String>>> {
    let height = df.height();
    let width = df.width();
    let cols = df.get_columns();

    let mut grid = vec![vec![String::new(); width]; height];
    for (row_index, row_values) in grid.iter_mut().enumerate() {
        for (col_index, cell_value) in row_values.iter_mut().enumerate() {
            let value = cols[col_index].get(row_index).map_err(|err| {
                PyValueError::new_err(format!("Failed to read header cell value: {err}"))
            })?;
            *cell_value = format_header_text_from_any_value(value);
        }
    }

    Ok(grid)
}

fn format_header_text_from_any_value(value: AnyValue<'_>) -> String {
    match value {
        AnyValue::Null => String::new(),
        AnyValue::String(val) => val.to_string(),
        AnyValue::StringOwned(val) => val.to_string(),
        _ => value.to_string(),
    }
}

fn parse_cell_format_patch(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<CellFormatPatch>> {
    let Some(obj) = obj else {
        return Ok(None);
    };
    if obj.is_none() {
        return Ok(None);
    }

    Ok(Some(CellFormatPatch {
        font_name: extract_optional_attr::<String>(obj, "font_name")?,
        font_size: extract_optional_attr::<i64>(obj, "font_size")?,
        bold: extract_optional_attr::<bool>(obj, "bold")?,
        italic: extract_optional_attr::<bool>(obj, "italic")?,
        align: extract_optional_attr::<String>(obj, "align")?,
        valign: extract_optional_attr::<String>(obj, "valign")?,
        border: extract_optional_attr::<i64>(obj, "border")?,
        text_wrap: extract_optional_attr::<bool>(obj, "text_wrap")?,
        top: extract_optional_attr::<i64>(obj, "top")?,
        bottom: extract_optional_attr::<i64>(obj, "bottom")?,
        left: extract_optional_attr::<i64>(obj, "left")?,
        right: extract_optional_attr::<i64>(obj, "right")?,
        num_format: extract_optional_attr::<String>(obj, "num_format")?,
        bg_color: extract_optional_attr::<String>(obj, "bg_color")?,
        font_color: extract_optional_attr::<String>(obj, "font_color")?,
    }))
}

fn parse_xlsx_write_options(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<XlsxWriteOptions>> {
    let Some(obj) = obj else {
        return Ok(None);
    };
    if obj.is_none() {
        return Ok(None);
    }

    let mut cfg_options_write = create_default_xlsx_write_options();

    if let Some(value_policy_obj) = extract_optional_attr_bound(obj, "value_policy")? {
        let mut value_policy = XlsxValuePolicy::default();
        if let Some(v) = extract_optional_attr::<String>(&value_policy_obj, "missing_value_str")? {
            value_policy.missing_value_str = v;
        }
        if let Some(v) = extract_optional_attr::<String>(&value_policy_obj, "nan_str")? {
            value_policy.nan_str = v;
        }
        if let Some(v) = extract_optional_attr::<String>(&value_policy_obj, "posinf_str")? {
            value_policy.posinf_str = v;
        }
        if let Some(v) = extract_optional_attr::<String>(&value_policy_obj, "neginf_str")? {
            value_policy.neginf_str = v;
        }
        if let Some(v) = extract_optional_attr::<String>(&value_policy_obj, "integer_coerce")? {
            value_policy.integer_coerce = if v == "coerce" {
                IntegerCoerceMode::Coerce
            } else {
                IntegerCoerceMode::Strict
            };
        }
        cfg_options_write.value_policy = value_policy;
    }

    if let Some(v) = extract_optional_attr::<bool>(obj, "should_keep_missing_values")? {
        cfg_options_write.should_keep_missing_values = v;
    }
    if let Some(v) = extract_optional_attr::<bool>(obj, "should_use_zip64")? {
        cfg_options_write.should_use_zip64 = v;
    }
    if let Some(v) = extract_optional_attr::<bool>(obj, "should_infer_numeric_cols")? {
        cfg_options_write.should_infer_numeric_cols = v;
    }
    if let Some(v) = extract_optional_attr::<bool>(obj, "should_infer_integer_cols")? {
        cfg_options_write.should_infer_integer_cols = v;
    }

    if let Some(row_chunk_policy_obj) = extract_optional_attr_bound(obj, "row_chunk_policy")? {
        if let Some(v) = extract_optional_attr::<usize>(&row_chunk_policy_obj, "width_large")? {
            cfg_options_write.row_chunk_policy.width_large = v;
        }
        if let Some(v) = extract_optional_attr::<usize>(&row_chunk_policy_obj, "width_medium")? {
            cfg_options_write.row_chunk_policy.width_medium = v;
        }
        if let Some(v) = extract_optional_attr::<usize>(&row_chunk_policy_obj, "size_large")? {
            cfg_options_write.row_chunk_policy.size_large = v;
        }
        if let Some(v) = extract_optional_attr::<usize>(&row_chunk_policy_obj, "size_medium")? {
            cfg_options_write.row_chunk_policy.size_medium = v;
        }
        if let Some(v) = extract_optional_attr::<usize>(&row_chunk_policy_obj, "size_default")? {
            cfg_options_write.row_chunk_policy.size_default = v;
        }
        if let Some(v) = extract_optional_attr::<usize>(&row_chunk_policy_obj, "fixed_size")? {
            cfg_options_write.row_chunk_policy.fixed_size = Some(v);
        }
    }

    Ok(Some(cfg_options_write))
}

fn parse_autofit_mode(value: &str) -> PyResult<AutofitMode> {
    match value {
        "none" => Ok(AutofitMode::None),
        "header" => Ok(AutofitMode::Header),
        "body" => Ok(AutofitMode::Body),
        "all" => Ok(AutofitMode::All),
        _ => Err(PyValueError::new_err(format!(
            "{PY_ARG_POLICY_AUTOFIT}.mode must be one of: 'none', 'header', 'body', 'all'."
        ))),
    }
}

fn parse_rule_scientific_scope(value: &str) -> PyResult<ScientificScope> {
    match value {
        "none" => Ok(ScientificScope::None),
        "decimal" => Ok(ScientificScope::Decimal),
        "integer" => Ok(ScientificScope::Integer),
        "all" => Ok(ScientificScope::All),
        _ => Err(PyValueError::new_err(format!(
            "{PY_ARG_POLICY_SCIENTIFIC}.scope must be one of: 'none', 'decimal', 'integer', 'all'."
        ))),
    }
}

fn parse_autofit_policy(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<AutofitPolicy>> {
    let Some(obj) = obj else {
        return Ok(None);
    };
    if obj.is_none() {
        return Ok(None);
    }

    let mut policy = AutofitPolicy::default();

    if let Some(v) = extract_optional_attr::<String>(obj, "mode")? {
        policy.mode = parse_autofit_mode(&v)?;
    }
    if obj.hasattr("max_rows")? {
        let val = obj.getattr("max_rows")?;
        if val.is_none() {
            policy.height_body_inferred_max = None;
        } else {
            policy.height_body_inferred_max = Some(val.extract::<usize>()?);
        }
    }
    if let Some(v) = extract_optional_attr::<usize>(obj, "min_width")? {
        policy.width_cell_min = v;
    }
    if let Some(v) = extract_optional_attr::<usize>(obj, "max_width")? {
        policy.width_cell_max = v;
    }
    if let Some(v) = extract_optional_attr::<usize>(obj, "padding")? {
        policy.width_cell_padding = v;
    }

    Ok(Some(policy))
}

fn parse_scientific_policy(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<ScientificPolicy>> {
    let Some(obj) = obj else {
        return Ok(None);
    };
    if obj.is_none() {
        return Ok(None);
    }

    let mut policy = ScientificPolicy::default();

    if let Some(v) = extract_optional_attr::<String>(obj, "scope")? {
        policy.scope = parse_rule_scientific_scope(&v)?;
    }
    if let Some(v) = extract_optional_attr::<f64>(obj, "min_absolute")? {
        policy.thr_min = v;
    }
    if let Some(v) = extract_optional_attr::<f64>(obj, "max_absolute")? {
        policy.thr_max = v;
    }
    Ok(Some(policy))
}

fn parse_column_refs(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<ColumnIdentifier>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_none() {
        return Ok(None);
    }

    if let Ok(b) = value.extract::<bool>() {
        if !b {
            return Ok(None);
        }
        return Err(PyValueError::new_err(
            "Column refs must be str, int, sequence[str | int], False, or None.",
        ));
    }

    if let Ok(c_value) = value.extract::<String>() {
        return Ok(Some(vec![ColumnIdentifier::Name(c_value)]));
    }
    if let Ok(idx) = value.extract::<usize>() {
        return Ok(Some(vec![ColumnIdentifier::Index(idx)]));
    }

    if let Ok(iter) = value.try_iter() {
        let mut refs = Vec::new();
        for item in iter {
            let item = item?;
            refs.push(parse_single_column_ref(&item)?);
        }
        return Ok(Some(refs));
    }

    Err(PyValueError::new_err(
        "Column refs must be str, int, sequence[str | int], False, or None.",
    ))
}

fn parse_single_column_ref(value: &Bound<'_, PyAny>) -> PyResult<ColumnIdentifier> {
    if let Ok(b) = value.extract::<bool>() {
        return Err(PyValueError::new_err(format!(
            "Column ref items must be str or int, got bool {b}."
        )));
    }
    if let Ok(name) = value.extract::<String>() {
        return Ok(ColumnIdentifier::Name(name));
    }
    if let Ok(idx) = value.extract::<usize>() {
        return Ok(ColumnIdentifier::Index(idx));
    }
    Err(PyValueError::new_err(
        "Column ref items must be str or int.",
    ))
}

fn convert_to_polars_dataframe<'py>(
    py: Python<'py>,
    df: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let module_polars = py.import("polars")?;
    let cls_dataframe = module_polars.getattr("DataFrame")?;

    if df.is_instance(&cls_dataframe)? {
        return Ok(df.clone());
    }

    cls_dataframe.call1((df,))
}

fn extract_optional_attr<T>(obj: &Bound<'_, PyAny>, attr: &str) -> PyResult<Option<T>>
where
    for<'a> T: FromPyObject<'a>,
{
    if !obj.hasattr(attr)? {
        return Ok(None);
    }
    let val = obj.getattr(attr)?;
    if val.is_none() {
        return Ok(None);
    }
    Ok(Some(val.extract::<T>()?))
}

fn extract_optional_attr_bound<'py>(
    obj: &Bound<'py, PyAny>,
    attr: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    if !obj.hasattr(attr)? {
        return Ok(None);
    }
    let val = obj.getattr(attr)?;
    if val.is_none() {
        return Ok(None);
    }
    Ok(Some(val))
}

#[pyfunction(name = "_profile_arrow_drain")]
fn profile_arrow_drain_py<'py>(
    _py: Python<'py>,
    source: &Bound<'py, PyAny>,
) -> PyResult<PyXlsxArrowDrainProfile> {
    let mut iter_batches = PyRecordBatchIter::from_arrow_stream_or_iterable(source)?;
    let mut batches = 0usize;
    let mut rows = 0usize;
    let mut cols: Option<usize> = None;

    for batch in &mut iter_batches {
        let batch = batch.map_err(PyRuntimeError::new_err)?;
        let width = batch.schema().len();
        if let Some(cols_seen) = cols {
            if cols_seen != width {
                return Err(PyValueError::new_err(
                    "All record batches must have identical column counts.",
                ));
            }
        } else {
            cols = Some(width);
        }
        batches += 1;
        rows += batch.len();
    }

    let cols = cols.unwrap_or(0);
    Ok(PyXlsxArrowDrainProfile {
        batches,
        rows,
        cols,
        cells: rows.saturating_mul(cols),
    })
}

pub fn register_xlsx_bindings(module: &Bound<'_, PyModule>) -> PyResult<()> {
    debug_assert!(!PY_VISIBLE_SYMBOLS.is_empty());
    module.add_class::<PyXlsxWriter>()?;
    module.add_class::<PyXlsxArrowDrainProfile>()?;
    module.add_function(wrap_pyfunction!(profile_arrow_drain_py, module)?)?;
    Ok(())
}

#[pymodule]
fn _native(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_xlsx_bindings(module)?;
    module.add("__bridge_abi__", BRIDGE_ABI_VERSION)?;
    module.add("__bridge_contract__", BRIDGE_CONTRACT_VERSION)?;
    module.add("__bridge_transport__", BRIDGE_TRANSPORT)?;
    module.add("__build_profile__", BUILD_PROFILE)?;
    Ok(())
}
