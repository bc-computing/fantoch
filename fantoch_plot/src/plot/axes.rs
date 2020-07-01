use pyo3::prelude::*;
use pyo3::types::PyDict;

pub struct Axes<'a> {
    ax: &'a PyAny,
}

impl<'a> Axes<'a> {
    pub fn new(ax: &'a PyAny) -> Self {
        Self { ax }
    }

    pub fn set_title(&self, title: &str) -> PyResult<()> {
        self.ax.call_method1("set_title", (title,))?;
        Ok(())
    }

    pub fn set_xlabel(&self, label: &str) -> PyResult<()> {
        self.ax.call_method1("set_xlabel", (label,))?;
        Ok(())
    }

    pub fn set_ylabel(&self, label: &str) -> PyResult<()> {
        self.ax.call_method1("set_ylabel", (label,))?;
        Ok(())
    }

    pub fn set_xticks<T>(&self, ticks: Vec<T>) -> PyResult<()>
    where
        T: IntoPy<PyObject>,
    {
        self.ax.call_method1("set_xticks", (ticks,))?;
        Ok(())
    }

    pub fn set_xticklabels<L>(&self, labels: Vec<L>) -> PyResult<()>
    where
        L: IntoPy<PyObject>,
    {
        self.ax.call_method1("set_xticklabels", (labels,))?;
        Ok(())
    }

    pub fn legend(&self, kwargs: Option<&PyDict>) -> PyResult<()> {
        self.ax.call_method("legend", (), kwargs)?;
        Ok(())
    }

    // TODO maybe take an optional `Fmt` struct instead
    pub fn plot<X, Y>(&self, x: Vec<X>, y: Vec<Y>, fmt: &str) -> PyResult<()>
    where
        X: IntoPy<PyObject>,
        Y: IntoPy<PyObject>,
    {
        self.ax.call_method1("plot", (x, y, fmt))?;
        Ok(())
    }

    pub fn bar<X, H>(
        &self,
        x: Vec<X>,
        height: Vec<H>,
        kwargs: Option<&PyDict>,
    ) -> PyResult<()>
    where
        X: IntoPy<PyObject>,
        H: IntoPy<PyObject>,
    {
        self.ax.call_method("bar", (x, height), kwargs)?;
        Ok(())
    }

    pub fn hist<X>(&self, x: Vec<X>) -> PyResult<()>
    where
        X: IntoPy<PyObject>,
    {
        self.ax.call_method1("hist", (x,))?;
        Ok(())
    }
}
